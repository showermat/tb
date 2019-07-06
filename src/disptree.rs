extern crate ncurses;

use std::rc::{Rc, Weak};
use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::time;
use keybinder::*;
use format::*;
use curses;
use curses::Output;
use ::errors::Result;

const COLWIDTH: usize = 4;
const RESERVED_FG_COLORS: usize = 2;

pub trait DispValue<'a> {
	fn placeholder(&self) -> FmtCmd;
	fn content(&self) -> FmtCmd;
	fn expandable(&self) -> bool;
	fn children(&self) -> Vec<Box<DispValue<'a> + 'a>>;
	fn invoke(&self);
}

pub trait DispSource<'a, V: DispValue<'a>> {
	//fn read<T: std::io::Read>(input: T) -> Result<Box<Self>>;
	fn root(&'a self) -> V;
	fn colors(&self) -> Vec<curses::Color>;
}

struct DispNodeCache {
	prefix0: String,
	prefix1: String,
	placeholder: Preformatted,
	content: Preformatted,
	search: Option<Search>,
}

struct DispNode<'a> {
	children: Vec<Rc<RefCell<DispNode<'a>>>>,
	parent: Weak<RefCell<DispNode<'a>>>,
	prev: Weak<RefCell<DispNode<'a>>>,
	next: Weak<RefCell<DispNode<'a>>>,
	prevsib: Weak<RefCell<DispNode<'a>>>,
	nextsib: Weak<RefCell<DispNode<'a>>>,
	index: usize,
	expanded: bool,
	last: bool,
	value: Box<DispValue<'a> + 'a>,
	cache: DispNodeCache,
}

fn weak_ptr_eq<T>(a: &Weak<T>, b: &Weak<T>) -> bool { // Shim for Weak::ptr_eq https://github.com/rust-lang/rust/issues/55981
	match (a.upgrade(), b.upgrade()) {
		(Some(x), Some(y)) => Rc::ptr_eq(&x, &y),
		(None, None) => true,
		_ => false,
	}
}

impl<'a> DispNode<'a> {
	fn depth(&self) -> usize {
		match self.parent.upgrade() {
			None => 0,
			Some(p) => p.borrow().depth() + 1,
		}
	}

	fn lines(&self) -> usize {
		match self.expanded {
			true => self.cache.placeholder.len(),
			false => self.cache.content.len(),
		}
	}

	fn path(&self) -> Vec<usize> {
		match self.parent.upgrade() {
			None => vec![],
			Some(p) => {
				let mut ret = p.borrow().path();
				ret.push(self.index);
				ret
			},
		}
	}

	fn root(this: &Rc<RefCell<DispNode<'a>>>) -> Rc<RefCell<DispNode<'a>>> {
		match this.borrow().parent.upgrade() {
			None => this.clone(),
			Some(p) => Self::root(&p),
		}
	}

	/* Things I dislike about Rust:
	 * Mein Gott!  This is an incredibly nasty syntax for doing a simple tree insertion.  In Java,
	 * Python, etc., the procedure would be a few fairly self-documenting pointer manipulations:
	 * `after.next.prev = n; n.next = after.next; if after.next.parent == n.parent:
	 * after.next.prevsib = n;` and so on.  The requirements for all the ref-count manipulation and
	 * borrow-scoping makes a lot uglier than it needs to be, and until I run it I have no idea
	 * whether I'll run into some issue with a borrow loop or something that will cause a panic.
	 */
	fn insert(after: &mut Rc<RefCell<DispNode<'a>>>, node: &mut Rc<RefCell<DispNode<'a>>>) {
		// FIXME Sibling links will be updated incorrectly if the node on each side is deeper in the tree than the one being inserted.
		let mut borrowed_node = node.borrow_mut();
		let mut borrowed_after = after.borrow_mut();
		if let Some(next) = borrowed_after.next.upgrade() {
			let mut borrowed_next = next.borrow_mut();
			borrowed_next.prev = Rc::downgrade(&node);
			borrowed_node.next = Rc::downgrade(&next);
			if weak_ptr_eq(&borrowed_next.parent, &borrowed_node.parent) {
				borrowed_next.prevsib = Rc::downgrade(&node);
			}
		}
		borrowed_after.next = Rc::downgrade(&node);
		borrowed_node.prev = Rc::downgrade(&after);
		borrowed_node.nextsib = borrowed_node.next.clone();
		borrowed_node.prevsib = borrowed_node.prev.clone();
		if !weak_ptr_eq(&Rc::downgrade(&after), &borrowed_node.parent) {
			borrowed_after.nextsib = Rc::downgrade(node);
		}
	}

	fn prefix(&self, maxdepth: usize, firstline: bool) -> String {
		fn repeat(s: &str, n: usize) -> String {
			std::iter::repeat(s).take(n).collect::<String>()
		}
		/* Things I dislike about Rust:
		 * You can't reference items from the environment in `fn`s, but you can't make recursive
		 * closures.  Oops, I guess I just need to pass around `maxdepth` in every function call
		 * and make everything look more complicated than it really is.
		 */
		fn parent_prefix(n: &DispNode, depth: usize, maxdepth: usize) -> String {
			if n.parent.upgrade().is_none() || depth > maxdepth { "".to_string() }
			else {
				let parent = n.parent.upgrade().unwrap();
				if n.last { parent_prefix(&parent.borrow(), depth + 1, maxdepth) + &repeat(" ", COLWIDTH) }
				else { parent_prefix(&parent.borrow(), depth + 1, maxdepth) + "│" + &repeat(" ", COLWIDTH - 1) }
			}
		}
		fn cur_prefix(n: &DispNode, maxdepth: usize) -> String {
			match n.parent.upgrade() {
				None => "".to_string(),
				Some(parent) => {
					let branch = if n.last { "└".to_string() } else { "├".to_string() };
					parent_prefix(&parent.borrow(), 1, maxdepth) + &branch + &repeat("─", COLWIDTH - 2) + " "
				}
			}
		}
		match firstline {
			true => cur_prefix(self, maxdepth),
			false => parent_prefix(self, 0, maxdepth),
		}
	}

	fn reformat(&mut self, screenwidth: usize) {
		assert!(screenwidth > 0);
		let maxdepth = if self.depth() == 0 { 0 } else { (self.depth() - 1) % ((screenwidth - 1) / COLWIDTH) };
		self.cache.prefix0 = self.prefix(maxdepth, true);
		self.cache.prefix1 = self.prefix(maxdepth, false);
		let contentw = screenwidth - ((maxdepth + 1) * COLWIDTH) % screenwidth;
		self.cache.content = self.value.content().format(contentw, RESERVED_FG_COLORS);
		self.cache.placeholder = self.value.placeholder().format(contentw, RESERVED_FG_COLORS);
	}

	fn new(parent: Weak<RefCell<DispNode<'a>>>, val: Box<DispValue<'a> + 'a>, width: usize, index: usize, last: bool) -> Self {
		let mut ret = DispNode {
			children: vec![],
			parent: parent,
			prev: Weak::new(),
			next: Weak::new(),
			prevsib: Weak::new(),
			nextsib: Weak::new(),
			index: index,
			expanded: false,
			last: last,
			value: val,
			cache: DispNodeCache {
				prefix0: "".to_string(),
				prefix1: "".to_string(),
				placeholder: Preformatted::new(0),
				content: Preformatted::new(0),
				search: None,
			},
		};
		ret.reformat(width);
		ret
	}

	fn new_root(val: Box<DispValue<'a> + 'a>, width: usize) -> Self {
		Self::new(Weak::new(), val, width, 0, true)
	}

	fn expandable(&self) -> bool {
		self.value.expandable()
	}

	fn expand(this: &mut Rc<RefCell<DispNode<'a>>>, width: usize) {
		if this.borrow().expandable() && !this.borrow().expanded {
			if this.borrow().value.children().len() > 0 {
				let mut cur = this.clone();
				let lastidx = this.borrow().value.children().len() - 1;
				let children = this.borrow().value.children();
				for (i, child) in children.into_iter().enumerate() {
					let mut node = Rc::new(RefCell::new(Self::new(Rc::downgrade(this), child, width, i, i == lastidx)));
					this.borrow_mut().children.push(node.clone());
					Self::insert(&mut cur, &mut node);
					cur = node;
				}
			}
			this.borrow_mut().expanded = true;
		}
	}

	fn collapse(this: &mut Rc<RefCell<DispNode>>) {
		let expanded = this.borrow().expanded;
		if expanded {
			if let Some(next) = this.borrow().next.upgrade() {
				next.borrow_mut().prev = Rc::downgrade(this);
			}
			if let Some(next) = this.borrow().nextsib.upgrade() {
				next.borrow_mut().prev = Rc::downgrade(this);
			}
			let mut mut_this = this.borrow_mut();
			mut_this.next = mut_this.nextsib.clone();
			mut_this.children = vec![];
			mut_this.expanded = false;
		}
	}

	fn toggle(this: &mut Rc<RefCell<DispNode<'a>>>, width: usize) {
		let expanded = this.borrow().expanded;
		match expanded {
			true => Self::collapse(this),
			false => Self::expand(this, width),
		}
	}

	fn recursive_expand(this: &mut Rc<RefCell<DispNode<'a>>>, width: usize) {
		if this.borrow().expandable() {
			if !this.borrow().expanded { Self::expand(this, width); }
			let mut children = this.borrow_mut().children.clone(); // `clone` necessary to prevent a runtime borrow loop
			for child in children.iter_mut() { Self::recursive_expand(child, width); }
		}
	}

	fn drawline(&self, palette: &curses::Palette, line: usize, selected: bool) {
		let prefixstr = match line {
			0 => &self.cache.prefix0,
			_ => &self.cache.prefix1,
		};
		let prefix = vec![Output::Fg(1), Output::Str(prefixstr.to_string())];
		let bg = match selected {
			true => 1,
			false => 0,
		};
		match self.expanded {
			true => self.cache.placeholder.write(line, palette, prefix, bg, &self.cache.search),
			false => self.cache.content.write(line, palette, prefix, bg, &self.cache.search),
		};
	}

	fn search(&mut self, query: &str) {
		let fmt = match self.expanded {
			true => &self.cache.placeholder,
			false => &self.cache.content,
		};
		if query != "" {
			if self.cache.search.is_none() || self.cache.search.as_ref().unwrap().query() != *query {
				self.cache.search = Some(fmt.search(query));
			}
		}
		else if self.cache.search.is_some() {
			self.cache.search = None;
		}
	}

	// TODO iterator search_from
	
	fn is_before(&self, n: Rc<RefCell<DispNode>>) -> bool {
		let (path1, path2) = (self.path(), n.borrow().path());
		for i in 0..=std::cmp::max(path1.len(), path2.len()) {
			if path2.len() <= i { return false; }
			if path1.len() <= i { return true; }
			if path1[i] > path2[i] { return false; }
			if path1[i] < path2[i] { return true; }
		}
		false
	}

	fn matchlines(&self) -> Vec<usize> {
		self.cache.search.as_ref().unwrap().matchlines()
	}
}

impl<'a> std::fmt::Debug for DispNode<'a> {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		let content = self.value.content().format(0 ,RESERVED_FG_COLORS).raw();
		write!(f, "DispNode({})", content)
	}
}

#[derive(Clone)]
struct DispPos<'a> {
	pub node: Weak<RefCell<DispNode<'a>>>,
	pub line: usize,
}

impl<'a> DispPos<'a> {
	fn new(node: Weak<RefCell<DispNode<'a>>>, line: usize) -> Self {
		DispPos { node: node, line: line }
	}

	fn nil() -> Self {
		DispPos { node: Weak::new(), line: 0 }
	}

	// The following three functions, while more elegantly written recursively, lead to stack overflows in large trees
	fn dist_fwd(&self, to: DispPos<'a>) -> Option<usize> {
		let mut ret = 0;
		let mut cur = self.clone();
		while !weak_ptr_eq(&cur.node, &to.node) {
			match cur.node.upgrade() {
				None => return None,
				Some(n) => {
					ret += n.borrow().lines() - cur.line;
					cur = DispPos::new(n.borrow().next.clone(), 0);
				},
			}
		}
		Some(ret + to.line - cur.line)
	}

	fn fwd(&self, n: usize, safe: bool) -> Self {
		let mut cur = self.clone();
		let mut remain = n;
		loop {
			match cur.node.upgrade() {
				None => return DispPos::nil(),
				Some(node) => {
					if remain < node.borrow().lines() - cur.line { break; }
					if node.borrow().next.upgrade().is_none() {
						match safe {
							false => return DispPos::nil(),
							true => return DispPos::new(cur.node, node.borrow().lines() - 1),
						}
					}
					remain -= node.borrow().lines() - cur.line;
					cur = DispPos::new(node.borrow().next.clone(), 0);
				}
			}
		}
		DispPos::new(cur.node, cur.line + remain)
	}

	fn bwd(&self, n: usize, safe: bool) -> Self {
		let mut cur = self.clone();
		let mut remain = n;
		loop {
			match cur.node.upgrade() {
				None => return DispPos::nil(),
				Some(node) => {
					if remain <= cur.line { break; }
					match node.borrow().prev.upgrade() {
						None => {
							match safe {
								false => return DispPos::nil(),
								true => return DispPos::new(cur.node, 0),
							}
						},
						Some(prev) => {
							remain -= cur.line + 1;
							cur = DispPos::new(node.borrow().prev.clone(), prev.borrow().lines() - 1)
						}
					}
				}
			}
		}
		DispPos::new(cur.node, cur.line - remain)
	}

	fn seek(&self, n: isize, safe: bool) -> Self {
		if n > 0 { self.fwd(n as usize, safe) }
		else if n < 0 { self.bwd(-n as usize, safe) }
		else { self.clone() }
	}
}

pub struct DispTree<'a> {
	root: Rc<RefCell<DispNode<'a>>>,
	sel: Weak<RefCell<DispNode<'a>>>,
	size: curses::Size,
	start: DispPos<'a>,
	offset: isize,
	down: bool,
	query: String,
	searchhist: Vec<String>,
	lastclick: time::Instant,
	numbuf: Vec<char>,
	palette: curses::Palette,
}

impl<'a> DispTree<'a> {
	pub fn new(json: Box<DispValue<'a> + 'a>, colors: Vec<curses::Color>) -> Self {
		let size = curses::scrsize();
		let mut root = Rc::new(RefCell::new(DispNode::new_root(json, size.w)));
		let mut fgcol = vec![ // RESERVED_FG_COLORS always needs to reflect this
			curses::Color { c8: 7, c256: 7 }, // regular
			curses::Color { c8: 4, c256: 244 }, // muted
		];
		fgcol.extend(colors);
		let bgcol = vec![
			curses::Color { c8: 0, c256: 0 }, // regular
			curses::Color { c8: 7, c256: 237 }, // selected
			curses::Color { c8: 3, c256: 88 }, // highlighted
		];
		let palette = curses::Palette::new(fgcol, bgcol);
		DispNode::toggle(&mut root, size.w);
		DispTree {
			sel: Rc::downgrade(&root),
			size: size,
			start: DispPos::new(Rc::downgrade(&root), 0),
			offset: 0,
			down: true,
			query: "".to_string(),
			searchhist: vec![],
			lastclick: time::Instant::now().checked_sub(time::Duration::from_secs(60)).unwrap(), // Epoch would be better
			numbuf: vec![],
			palette: palette,
			root: root,
		}
	}

	fn last(&self) -> Rc<RefCell<DispNode<'a>>> {
		let mut cur = self.root.clone();
		loop {
			let new =
				if let Some(nextsib) = cur.borrow().nextsib.upgrade() { Some(nextsib) }
				else if let Some(next) = cur.borrow().next.upgrade() { Some(next) }
				else { None };
			if let Some(n) = new { cur = n; }
			else { break; }
		}
		cur
	}

	fn check_term_size(&self) -> bool {
		if self.size.h < 1 || self.size.w < 24 {
			ncurses::mvaddstr(0, 0, "Terminal too small!");
			false
		}
		else { true }
	}

	fn drawline(&self, line: usize, cur: DispPos<'a>) {
		const DEBUG: bool = false;
		if self.check_term_size() {
			ncurses::mv(line as i32, 0);
			ncurses::clrtoeol();
			let selected = weak_ptr_eq(&self.sel, &cur.node);
			if let Some(node) = cur.node.upgrade() {
				if DEBUG {
					let fill = std::iter::repeat(" ").take(self.size.w).collect::<String>();
					ncurses::attron(ncurses::A_REVERSE());
					ncurses::addstr(&fill);
					ncurses::refresh();
					std::thread::sleep(time::Duration::from_millis(100));
					ncurses::mv(line as i32, 0);
					ncurses::attroff(ncurses::A_REVERSE());
					ncurses::addstr(&fill);
					ncurses::mv(line as i32, 0);
				}
				node.borrow_mut().search(&self.query);
				node.borrow().drawline(&self.palette, cur.line, selected);
			}
		}
	}

	fn drawlines(&self, lines: (usize, usize)) {
		let mut cur = self.start.fwd(lines.0, false);
		for i in lines.0..lines.1 {
			self.drawline(i, cur.clone());
			cur = cur.fwd(1, false);
		}
	}

	fn sellines(&self) -> (usize, usize) {
		assert!(self.offset >= 0);
		let offset = self.offset as usize;
		let sel = self.sel.upgrade().unwrap();
		let lines = sel.borrow().lines();
		match self.down {
			false => (offset, std::cmp::min(offset + lines, self.size.h)),
			true => (std::cmp::max(offset as isize - lines as isize + 1, 0) as usize, offset + 1),
		}
	}

	fn statline(&self) {
		let writeat = |x: usize, s: &str| {
			if s != "" { ncurses::mvaddstr(self.size.h as i32, (x + 1) as i32, s); }
		};
		if self.check_term_size() {
			ncurses::mv(self.size.h as i32, 0);
			ncurses::clrtoeol();
			writeat(self.size.w - 8, &self.numbuf.iter().collect::<String>());
		}
	}

	fn scroll(&mut self, by: isize) -> isize {
		match self.check_term_size() {
			false => 0,
			true => {
				let oldsel = self.sel.clone();
				let newstart = self.start.seek(by, true);
				let diff = match by {
					i if i > 0 => self.start.dist_fwd(newstart.clone()).unwrap() as isize,
					i if i < 0 => -(newstart.dist_fwd(self.start.clone()).unwrap() as isize),
					_ => 0,
				};
				let dist = diff.abs() as usize;
				self.start = newstart;
				self.offset -= diff;
				if by > 0 {
					while self.offset < 0 {
						match self.down {
							false => {
								let sel = self.sel.upgrade().unwrap();
								self.offset += (sel.borrow().lines() - 1) as isize;
								self.down = true
							}
							true => {
								let sel1 = self.sel.upgrade().unwrap();
								self.sel = sel1.borrow().next.clone();
								let sel2 = self.sel.upgrade().unwrap();
								self.offset += sel2.borrow().lines() as isize;
							}
						}
					}
				}
				else {
					while self.offset >= self.size.h as isize {
						match self.down {
							true => {
								let sel = self.sel.upgrade().unwrap();
								self.offset -= (sel.borrow().lines() - 1) as isize;
								self.down = false;
							}
							false => {
								let sel1 = self.sel.upgrade().unwrap();
								self.sel = sel1.borrow().prev.clone();
								let sel2 = self.sel.upgrade().unwrap();
								self.offset -= sel2.borrow().lines() as isize;
							}
						}
					}
				}
				if dist >= self.size.h { self.drawlines((0, self.size.h)); }
				else if diff != 0 {
					if diff > 0 {
						ncurses::scrl(dist as i32);
						self.drawlines((self.size.h - dist, self.size.h));
					}
					else if diff < 0 {
						ncurses::scrl(-(dist as i32));
						self.drawlines((0, dist));
					}
					if !weak_ptr_eq(&self.sel, &oldsel) { self.drawlines(self.sellines()); }
				}
				self.statline();
				diff
			},
		}
	}

	fn select(&mut self, sel: Rc<RefCell<DispNode<'a>>>) -> isize {
		let oldsel = self.sel.upgrade().unwrap();
		match self.check_term_size() {
			false => 0,
			true => {
				let down = oldsel.borrow().is_before(sel.clone());
				let oldlines = self.sellines();
				let curpos = match self.down {
					true => oldsel.borrow().lines() - 1,
					false => 0,
				};
				match down {
					true => self.offset += DispPos::new(self.sel.clone(), curpos).dist_fwd(DispPos::new(Rc::downgrade(&sel), sel.borrow().lines() - 1)).unwrap() as isize,
					false => self.offset -= DispPos::new(Rc::downgrade(&sel), 0).dist_fwd(DispPos::new(self.sel.clone(), curpos)).unwrap() as isize,
				};
				self.down = down;
				self.sel = Rc::downgrade(&sel);
				let mut scrolldist = 0;
				if self.offset < 0 { let sd = self.offset; scrolldist = self.scroll(sd); }
				else if self.offset >= self.size.h as isize { let sd = self.offset - self.size.h as isize + 1; scrolldist = self.scroll(sd); }
				else { self.statline(); }
				if oldlines.0 as isize - scrolldist < self.size.h as isize && oldlines.1 as isize - scrolldist >= 0 {
					self.drawlines((std::cmp::max(oldlines.0 as isize - scrolldist, 0) as usize, std::cmp::min(oldlines.1 as isize - scrolldist, self.size.h as isize) as usize)); // Clear the old selection
				}
				if (scrolldist.abs() as usize) < self.size.h {
					let mut sellines = self.sellines();
					if scrolldist > 0 { sellines = (std::cmp::min(sellines.0, self.size.h - scrolldist as usize), std::cmp::min(sellines.1, self.size.h - scrolldist as usize)); }
					else if scrolldist < 0 { sellines = (std::cmp::max(sellines.0, -scrolldist as usize), std::cmp::max(sellines.1, -scrolldist as usize)); }
					if sellines.0 < sellines.1 { self.drawlines(sellines); }
				}
				scrolldist
			},
		}
	}

	fn foreach(&mut self, f: &Fn(&mut DispNode)) {
		let mut cur = Rc::downgrade(&self.root);
		while let Some(n) = cur.upgrade() {
			f(&mut n.borrow_mut());
			cur = n.borrow().next.clone();
		}
	}

	fn refresh_offset(&mut self) {
		let sel = self.sel.upgrade().unwrap();
		let line = match self.down {
			false => 0,
			true => sel.borrow().lines() - 1,
		};
		self.offset = self.start.dist_fwd(DispPos::new(self.sel.clone(), line)).unwrap() as isize;
	}

	fn refresh(&self) {
		self.drawlines((0, self.size.h));
		self.statline();
	}

	fn resize(&mut self) {
		let mut size = curses::scrsize();
		size.h -= 1;
		self.size = size;
		if self.check_term_size() {
			let w = self.size.w;
			self.foreach(&|n: &mut DispNode| n.reformat(w));
			let sel = self.sel.upgrade().unwrap();
			self.refresh_offset();
			self.select(sel); // TODO This causes an unnecessary redraw of the selection that we should try to avoid
			self.refresh();
		}
	}

	fn selpos(&mut self, line: usize) {
		let target = self.start.fwd(line, true).node.upgrade().unwrap();
		self.select(target);
	}

	fn adjust_offset(&mut self, op: &Fn(&mut Rc<RefCell<DispNode>>) -> ()) {
		let mut maxend = 0;
		let mut sel = self.sel.upgrade().unwrap();
		let lines_before = sel.borrow().lines() as isize;
		if sel.borrow().expanded { maxend = DispPos::new(Rc::downgrade(&sel), 0).dist_fwd(DispPos::nil()).unwrap(); }
		op(&mut sel);
		let lines_after = sel.borrow().lines() as isize;
		if sel.borrow().expanded { maxend = DispPos::new(Rc::downgrade(&sel), 0).dist_fwd(DispPos::nil()).unwrap(); }
		if self.down { self.offset += lines_after - lines_before; }
		let drawstart = match self.down {
			true => self.offset - lines_after + 1,
			false => self.offset,
		};
		// Unfortunately we need to redraw the whole selection, because we don't know how much it's changed because of the (un)expansion.
		self.drawlines((drawstart as usize, std::cmp::min(self.size.h, self.offset as usize + maxend)));
	}

	fn togglesel(&mut self) {
		let w = self.size.w;
		self.adjust_offset(&|mut sel| DispNode::toggle(&mut sel, w));
	}

	fn recursive_expand(&mut self) {
		let w = self.size.w;
		self.adjust_offset(&|mut sel| DispNode::recursive_expand(&mut sel, w));
	}

	fn setquery(&mut self, query: &str) {
		self.query = query.to_string();
		let mut redraw: HashMap<usize, DispPos> = HashMap::new();
		let mut cur = self.start.clone().node.upgrade().unwrap();
		let mut line = -(self.start.line as isize);
		let onscreen = |i: isize| i >= 0 && i < self.size.h as isize;
		while line < self.size.h as isize {
			if let Some(search) = cur.borrow().cache.search.as_ref() {
				for m in search.matchlines() {
					let matchline = line + m as isize;
					if onscreen(matchline) {
						redraw.insert(matchline as usize, DispPos::new(Rc::downgrade(&cur), m));
					}
				}
			}
			cur.borrow_mut().search(&self.query);
			if self.query != "" {
				for m in cur.borrow().cache.search.as_ref().unwrap().matchlines() { // Unwrap asserts `query` cannot be empty after calling `search()`
					let matchline = line + m as isize;
					if onscreen(matchline) {
						redraw.insert(matchline as usize, DispPos::new(Rc::downgrade(&cur), m));
					}
				}
			}
			line += cur.borrow().lines() as isize;
			let next = cur.borrow().next.upgrade();
			match next {
				None => break,
				Some(n) => cur = n,
			}
		}
		for (line, pos) in redraw {
			self.drawline(line, pos);
		}
	}

	fn searchnext(&mut self, offset: isize) {
		unimplemented!("searchnext");
	}

	fn search(&mut self, forward: bool) {
		if self.check_term_size() {
			let oldquery = self.query.clone();
			self.setquery("");
			let incsearch = Box::new(|dt: &mut DispTree, q: &str| dt.setquery(q));
			let size = self.size; // For borrowing
			let palette = self.palette.clone();
			let searchhist = self.searchhist.clone(); // Any way to avoid these expensive clones?
			let res = ::prompt::prompt(self, (size.h, 0), size.w - 20, if forward { "/" } else { "?" }, "", searchhist, incsearch, &palette);
			if res == "" {
				self.setquery(&oldquery);
			}
			else {
				self.searchhist.push(res);
				//self.searchnext(if forward { 1 } else { -1 });
			}
		}
	}

	fn click(&mut self, y: usize) {
		let now = time::Instant::now();
		let oldsel = self.sel.clone();
		self.selpos(y);
		if weak_ptr_eq(&oldsel, &self.sel) && now.duration_since(self.lastclick).as_millis() < 400 {
			self.togglesel();
			self.lastclick = now.checked_sub(time::Duration::from_secs(60)).unwrap(); // Epoch would be better
		}
		else { self.lastclick = now; }
	}

	fn mouse(&mut self, events: Vec<curses::MouseEvent>) {
		use curses::MouseClick::*;
		for event in events {
			match (event.button, event.kind) {
				(1, Click) => self.click(event.y as usize),
				(1, DoubleClick) => self.togglesel(), // This doesn't work in my terminal; we get two separate click events
				(4, Press) => { self.scroll(-4); },
				(5, Press) => { self.scroll(4); },
				_ => (),
			}
		}
	}

	fn addnum(&mut self, n: char) {
		if n != '0' || !self.numbuf.is_empty() {
			while self.numbuf.len() >= 6 { self.numbuf.remove(0); }
			self.numbuf.push(n);
		}
		self.statline();
	}

	fn clearnum(&mut self) {
		self.numbuf = vec![];
		self.statline();
	}

	fn getnum(&self) -> usize {
		match self.numbuf.is_empty() {
			true => 1,
			false => self.numbuf.iter().collect::<String>().parse::<usize>().unwrap(),
		}
	}

	fn seek(&self, rel: &Fn(&Rc<RefCell<DispNode<'a>>>) -> Weak<RefCell<DispNode<'a>>>) -> Rc<RefCell<DispNode<'a>>> {
		let mut ret = self.sel.upgrade().unwrap();
		for _ in 1..=self.getnum() {
			let next = rel(&ret);
			if let Some(newret) = next.upgrade() { ret = newret; }
			else { break; }
		}
		ret
	}

	pub fn interactive(&mut self) {
		use curses::ncstr;
		let digits = (0..=9).map(|x| ncstr(&x.to_string())).collect::<Vec<Vec<i32>>>();
		let done = Arc::new(Mutex::new(false)); // TODO Don't use Arc and Mutex
		let d = done.clone();
		let mut keys: Keybinder<Self> = Keybinder::new();

		keys.register(&[&[ncurses::KEY_RESIZE]], Box::new(|dt, _| { dt.resize(); }));
		keys.register(&[&[ncurses::KEY_MOUSE]], Box::new(|dt, _| dt.mouse(curses::mouseevents())));
		keys.register(&[&ncstr(" ")], Box::new(|dt, _| dt.togglesel()));
		keys.register(&[&ncstr("x")], Box::new(|dt, _| dt.recursive_expand()));
		keys.register(&[&ncstr("\n")], Box::new(|dt, _| { let sel = dt.sel.upgrade().unwrap(); sel.borrow().value.invoke(); dt.refresh(); }));
		keys.register(&digits.iter().map(|x| &x[..]).collect::<Vec<&[i32]>>(), Box::new(|dt, digit| dt.addnum(digit[0] as u8 as char)));
		keys.register(&[&[0xc]], Box::new(|dt, _| dt.refresh())); // ^L
		keys.register(&[&ncstr("j"), &[ncurses::KEY_DOWN]], Box::new(|dt, _| { let sel = dt.seek(&|n: &Rc<RefCell<DispNode<'a>>>| n.borrow().next.clone()); dt.select(sel); }));
		keys.register(&[&ncstr("k"), &[ncurses::KEY_UP]], Box::new(|dt, _| { let sel = dt.seek(&|n: &Rc<RefCell<DispNode<'a>>>| n.borrow().prev.clone()); dt.select(sel); }));
		keys.register(&[&ncstr("J")], Box::new(|dt, _| { let sel = dt.seek(&|n: &Rc<RefCell<DispNode<'a>>>| n.borrow().nextsib.clone()); dt.select(sel); }));
		keys.register(&[&ncstr("K")], Box::new(|dt, _| { let sel = dt.seek(&|n: &Rc<RefCell<DispNode<'a>>>| n.borrow().prevsib.clone()); dt.select(sel); }));
		keys.register(&[&ncstr("p")], Box::new(|dt, _| { let sel = dt.seek(&|n: &Rc<RefCell<DispNode<'a>>>| n.borrow().parent.clone()); dt.select(sel); }));
		keys.register(&[&ncstr("g"), &[ncurses::KEY_HOME]], Box::new(|dt, _| { let sel = dt.root.clone(); dt.select(sel); }));
		keys.register(&[&ncstr("G"), &[ncurses::KEY_END]], Box::new(|dt, _| { let sel = dt.last(); dt.select(sel); }));
		keys.register(&[&ncstr("H")], Box::new(|dt, _| { dt.selpos(0); }));
		keys.register(&[&ncstr("M")], Box::new(|dt, _| { let pos = dt.size.h / 2; dt.selpos(pos); }));
		keys.register(&[&ncstr("L")], Box::new(|dt, _| { let pos = dt.size.h - 1; dt.selpos(pos); }));
		keys.register(&[&[0x6], &[ncurses::KEY_NPAGE]], Box::new(|dt, _| { let dist = dt.getnum() * dt.size.h; dt.scroll(dist as isize); })); // ^F
		keys.register(&[&[0x2], &[ncurses::KEY_PPAGE]], Box::new(|dt, _| { let dist = dt.getnum() * dt.size.h; dt.scroll(-(dist as isize)); })); // ^B
		keys.register(&[&[0x4]], Box::new(|dt, _| { let dist = dt.getnum() * dt.size.h / 2; dt.scroll(dist as isize); })); // ^D
		keys.register(&[&[0x15]], Box::new(|dt, _| { let dist = dt.getnum() * dt.size.h / 2; dt.scroll(-(dist as isize)); })); // ^U
		keys.register(&[&[0x5]], Box::new(|dt, _| { dt.scroll(1); })); // ^E
		keys.register(&[&[0x19]], Box::new(|dt, _| { dt.scroll(-1); })); // ^Y
		keys.register(&[&ncstr("zz")], Box::new(|dt, _| { let dist = dt.offset as isize - (dt.size.h as isize) / 2; dt.scroll(dist); }));
		keys.register(&[&ncstr("/")], Box::new(|dt, _| { dt.search(true); }));
		keys.register(&[&ncstr("?")], Box::new(|dt, _| { dt.search(false); }));
		keys.register(&[&ncstr("n")], Box::new(|dt, _| { let n = dt.getnum() as isize; dt.searchnext(n); }));
		keys.register(&[&ncstr("N")], Box::new(|dt, _| { let n = -(dt.getnum() as isize); dt.searchnext(n); }));
		keys.register(&[&ncstr("c")], Box::new(|dt, _| { dt.setquery(""); }));
		keys.register(&[&ncstr("q")], Box::new(move |_, _| { *d.lock().unwrap() = true; }));

		self.resize();
		while !*done.lock().unwrap() {
			let cmd = keys.wait(self);
			if !digits.contains(&cmd) { self.clearnum(); }
		}
	}
}
