use std::cell::RefCell;
use std::cmp;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::{Arc, Mutex};
use std::time;
use ::curses;
use ::errors::*;
use ::interface::*;
use ::keybinder::Keybinder;
use ::owning_ref::OwningHandle;
use ::regex::Regex;
use super::node::{Node, State};
use super::pos::Pos;
use super::statmsg::StatMsg;

type OwnedRoot<'a> = OwningHandle<Box<dyn Source>, Box<Rc<RefCell<Node<'a>>>>>;

struct TransformManager<'a> {
	base: OwnedRoot<'a>,
	cur: Option<OwnedRoot<'a>>,
	next: Option<OwnedRoot<'a>>,
}

impl<'a> TransformManager<'a> {
	fn new_owned_root(source: Box<dyn Source>, w: usize, hideroot: bool) -> OwnedRoot<'a> {
		OwningHandle::new_with_fn(source, |s| unsafe { Box::new(Rc::new(RefCell::new(Node::new_root(s.as_ref().unwrap().root(), w, hideroot)))) } )
	}

	pub fn new(source: Box<dyn Source>, w: usize, hideroot: bool) -> Self {
		Self {
			base: Self::new_owned_root(source, w, hideroot),
			cur: None,
			next: None,
		}
	}

	pub fn clear(&mut self) -> &Rc<RefCell<Node<'a>>> {
		self.next = None;
		self.cur = None;
		&*self.base
	}

	pub fn propose(&mut self, q: &str, w: usize, hideroot: bool) -> Result<&Rc<RefCell<Node<'a>>>> {
		match self.cur.as_ref().unwrap_or(&self.base).as_owner().transform(q) {
			Ok(tree) => {
				self.next = Some(Self::new_owned_root(tree, w, hideroot));
				Ok(&*(self.next.as_ref().unwrap()))
			},
			Err(error) => Err(error),
		}
	}

	pub fn accept(&mut self) {
		std::mem::swap(&mut self.cur, &mut self.next);
		self.next = None;
	}

	pub fn reject(&mut self) -> &Rc<RefCell<Node<'a>>> {
		self.next = None;
		&*(self.cur.as_ref().unwrap_or(&self.base))
	}
}

pub struct Tree<'a> {
	source: TransformManager<'a>, // Holds tree source and manages transformations
	root: Rc<RefCell<Node<'a>>>, // Root node of the displayed tree
	sel: Weak<RefCell<Node<'a>>>, // Currently selected node
	size: curses::Size, // Terminal size
	start: Pos<'a>, // Node and line corresponding to the top of the screen
	offset: isize, // Line number of currently selected node (distance from start to first line of sel)
	query: Option<Regex>, // Current search query
	searchhist: Vec<String>, // Past search queries
	xformhist: Vec<String>, // Past transformations
	searchfwd: bool, // Whether the user is searching forward or backward
	lastclick: time::Instant, // Time of the last click, for double-click detection
	numbuf: Vec<char>, // Buffer for numbers entered to prefix a command
	palette: curses::Palette, // Colors available for drawing this tree
	settings: Settings, // Configuration info
	lock: Arc<Mutex<()>>, // Single-thread all updates
}

impl<'a> Tree<'a> {
	pub fn new(tree: Box<dyn Source>, colors: Vec<Color>, settings: Settings) -> Result<Self> {
		let size = curses::scrsize();
		let mut source = TransformManager::new(tree, size.w, settings.hide_root);
		let root = Rc::clone(source.clear());
		let mut fgcol = super::FG_COLORS.to_vec();
		fgcol.extend(colors);
		let palette = curses::Palette::new(fgcol, super::BG_COLORS.to_vec())?;
		Ok(Tree {
			source: source,
			sel: Rc::downgrade(&root),
			size: size,
			start: Pos::new(Rc::downgrade(&root), 0),
			offset: 0,
			query: None,
			searchhist: vec![],
			xformhist: vec![],
			searchfwd: true,
			lastclick: time::Instant::now().checked_sub(time::Duration::from_secs(60)).expect("This program cannot be run before January 2, 1970"),
			numbuf: vec![],
			palette: palette,
			root: root,
			settings: settings,
			lock: Arc::new(Mutex::new(())),
		})
	}

	fn first(&self) -> Rc<RefCell<Node<'a>>> {
		let mut cur = self.root.clone();
		while cur.borrow().lines() == 0 {
			if let Some(next) = Node::next(&cur).upgrade() { cur = next; }
			else { return self.root.clone(); }
		}
		cur
	}

	fn last(&self) -> Rc<RefCell<Node<'a>>> {
		let mut cur = self.root.clone();
		loop {
			let new =
				if let Some(nextsib) = Node::nextsib(&cur).upgrade() { Some(nextsib) }
				else if let Some(next) = Node::next(&cur).upgrade() { Some(next) }
				else { None };
			if let Some(n) = new { cur = n; }
			else { break; }
		}
		cur
	}

	fn check_term_size(&self) -> bool {
		if self.size.h < 1 || self.size.w < 24 {
			ncurses::clear();
			ncurses::mvaddstr(0, 0, "Terminal too small!");
			false
		}
		else { true }
	}

	fn drawline(&self, line: usize, cur: Pos<'a>) {
		const DEBUG: bool = true;
		if self.check_term_size() {
			ncurses::mv(line as i32, 0);
			ncurses::clrtoeol();
			let selected = self.sel.ptr_eq(&cur.node);
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
		let sel = self.sel.upgrade().expect("Couldn't get selection in sellines");
		let lines = sel.borrow().lines();
		//assert!(self.offset + lines as isize >= 0 && self.offset < self.size.h as isize);
		(cmp::max(self.offset, 0) as usize, cmp::min((self.offset + lines as isize) as usize, self.size.h))
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
		if self.check_term_size() && by != 0 {
			let oldsel = self.sel.clone();
			let newstart = self.start.seek(by, true);
			let diff = match by {
				i if i > 0 => self.start.dist_fwd(newstart.clone()).expect("Seek returned an incorrect node") as isize,
				i if i < 0 => -(newstart.dist_fwd(self.start.clone()).expect("Seek returned an incorrect node") as isize),
				_ => 0,
			};
			let dist = diff.abs() as usize;
			self.start = newstart;
			self.offset -= diff;
			if by > 0 {
				loop {
					let sel = self.sel.upgrade().expect("Couldn't get selection in scroll");
					let lines = sel.borrow().lines() as isize;
					if self.offset + lines - 1 >= 0 { break; }
					self.offset += lines;
					self.sel = Node::next(&sel).clone();
				}
			}
			else {
				while self.offset >= self.size.h as isize {
					let oldsel = self.sel.upgrade().expect("Couldn't get selection in scroll");
					self.sel = Node::prev(&oldsel).clone();
					let newsel = self.sel.upgrade().expect("Couldn't get selection in scroll");
					self.offset -= newsel.borrow().lines() as isize;
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
				if !self.sel.ptr_eq(&oldsel) { self.drawlines(self.sellines()); }
			}
			self.statline();
			diff
		}
		else { 0 }
	}

	fn select(&mut self, sel: Rc<RefCell<Node<'a>>>, scrollin: bool) -> isize {
		if self.check_term_size() {
			let oldsel = self.sel.upgrade().expect("Couldn't get selection in select");
			let same = Rc::ptr_eq(&oldsel, &sel);
			let down = oldsel.borrow().is_before(sel.clone());
			let oldlines = self.sellines();
			self.offset += match down {
				true => Pos::new(self.sel.clone(), 0).dist_fwd(Pos::new(Rc::downgrade(&sel), 0))
					.expect("Down is true but new selection not after old") as isize,
				false => -(Pos::new(Rc::downgrade(&sel), 0).dist_fwd(Pos::new(self.sel.clone(), 0))
					.expect("Down is false but new selection not before old") as isize),
			};
			self.sel = Rc::downgrade(&sel);
			let scrolldist = self.scroll({
				let lines = sel.borrow().lines() as isize;
				let off = self.offset;
				let h = self.size.h as isize;
				if lines == 0 { if scrollin { self.statline(); } 0 }
				else if scrollin && off < 0 { off + lines - h - cmp::min(lines - h, 0) }
				else if scrollin && off + lines >= h { off + cmp::min(lines - h, 0) }
				else if off + lines <= 0 { off + lines - 1 }
				else if off >= h { off - h + 1 }
				else { if scrollin { self.statline(); } 0 }
			});
			if oldlines.0 as isize - scrolldist < self.size.h as isize && oldlines.1 as isize - scrolldist >= 0 && !same {
				self.drawlines(( // Clear the old selection
					cmp::max(oldlines.0 as isize - scrolldist, 0) as usize,
					cmp::min(oldlines.1 as isize - scrolldist, self.size.h as isize) as usize
				));
			}
			if (scrolldist.abs() as usize) < self.size.h {
				let mut sellines = self.sellines();
				if scrolldist > 0 {
					sellines = (cmp::min(sellines.0, self.size.h - scrolldist as usize), cmp::min(sellines.1, self.size.h - scrolldist as usize));
				}
				else if scrolldist < 0 {
					sellines = (cmp::max(sellines.0, -scrolldist as usize), cmp::max(sellines.1, -scrolldist as usize));
				}
				if sellines.0 < sellines.1 && !same { self.drawlines(sellines); }
			}
			scrolldist
		}
		else { 0 }
	}

	fn foreach(&mut self, f: &dyn Fn(&mut Node)) {
		let mut cur = Rc::downgrade(&self.root);
		while let Some(n) = cur.upgrade() {
			f(&mut n.borrow_mut());
			cur = Node::next(&n).clone();
		}
	}

	fn redraw(&self) {
		ncurses::clear();
		self.drawlines((0, self.size.h));
		self.statline();
	}

	fn resize(&mut self) {
		let mut size = curses::scrsize();
		size.h -= 1;
		self.size = size;
		if self.check_term_size() {
			let w = self.size.w;
			self.foreach(&|n: &mut Node| n.reformat(w));
			self.start = Pos::new(self.start.node.clone(), 0).fwd(self.start.line, true);
			let sel = self.sel.upgrade().expect("Couldn't get selection in resize");
			// If `start` is the last line of a multi-line wrapped node, but we make the terminal
			// wider and the node unwraps to fewer lines, `sel` will now be before `start`.
			let curpos = Pos::new(self.sel.clone(), 0);
			self.offset = {
				let fwd = self.start.dist_fwd(curpos.clone()).map(|x| x as isize);
				if let Some(ret) = fwd { ret }
				else { -(curpos.dist_fwd(self.start.clone()).expect("Could not determine new offset in resize") as isize) }
			};
			self.select(sel, false);
			self.redraw();
		}
	}

	fn selpos(&mut self, line: usize) {
		let target = self.start.fwd(line, true).node.upgrade().expect("Tried to select invalid line");
		self.select(target, true);
	}

	fn accordion(&mut self, mut node: &mut Rc<RefCell<Node<'a>>>, op: &dyn Fn(&mut Rc<RefCell<Node>>, usize) -> ()) {
		ncurses::refresh();
		let start = self.start.node.upgrade().expect("Couldn't get start node in accordion");
		let sel = self.sel.upgrade().expect("Couldn't get selection in accordion");
		if node.borrow().is_before(start.clone()) {
			if node.borrow().is_ancestor_of(start) {
				if node.borrow().is_ancestor_of(sel.clone()) {
					self.select(node.clone(), true); // TODO Use path resolution to select a new sel
					op(&mut node, self.size.w);
				}
				else {
					let oldoff = Pos::new(Rc::downgrade(&node), 0).dist_fwd(Pos::new(Rc::downgrade(&sel), 0));
					op(&mut node, self.size.w);
					let newoff = Pos::new(Rc::downgrade(&node), 0).dist_fwd(Pos::new(Rc::downgrade(&sel), 0));
					let diff = newoff.unwrap() as isize - oldoff.unwrap() as isize;
					self.offset += diff;
					self.start = Pos::new(Rc::downgrade(&sel), 0).seek(-self.offset, true);
					self.scroll(diff);
				}
				self.redraw();
			}
			else { op(&mut node, self.size.w); }
		}
		else if self.start.fwd(self.size.h - 1, true).node.upgrade().unwrap().borrow().is_before(node.clone()) { op(&mut node, self.size.w); }
		else if node.borrow().is_before(sel.clone()) {
			if node.borrow().is_ancestor_of(sel.clone()) {
				self.select(node.clone(), true); // TODO Use path resolution to select a new sel
				op(&mut node, self.size.w);
			}
			else {
				let oldoff = Pos::new(Rc::downgrade(&node), 0).dist_fwd(Pos::new(Rc::downgrade(&sel), 0));
				op(&mut node, self.size.w);
				let newoff = Pos::new(Rc::downgrade(&node), 0).dist_fwd(Pos::new(Rc::downgrade(&sel), 0));
				let diff = newoff.unwrap() as isize - oldoff.unwrap() as isize;
				self.offset += diff;
				self.scroll(diff);
			}
			self.redraw();
		}
		else {
			let mut maxend = Pos::new(Rc::downgrade(&node), 0).dist_fwd(Pos::nil()).expect("Failed to find distance to end of document");
			op(&mut node, self.size.w);
			maxend = cmp::max(maxend, Pos::new(Rc::downgrade(&node), 0).dist_fwd(Pos::nil()).expect("Failed to find distance to end of document"));
			// Unfortunately we need to redraw the whole selection, because we don't know how much it's changed due to the (un)expansion
			let startoff = cmp::max(self.offset, 0) as usize;
			self.drawlines((startoff, cmp::min(self.size.h, startoff + maxend)));
		}
	}

	fn refresh(&mut self, node: &mut Rc<RefCell<Node<'a>>>) {
		self.accordion(node, &|n, w| Node::refresh(n, w));
	}

	fn query_from_str(query: &str) -> Option<Regex> {
		match query {
			"" => None,
			q => Some(Regex::new(q).unwrap_or(Regex::new(&regex::escape(q)).expect("Regex construction failed even after escaping"))),
		}
	}

	fn setquery(&mut self, query: Option<Regex>) {
		self.query = query;
		let mut to_redraw: HashMap<usize, Pos> = HashMap::new();
		let mut cur = self.start.clone().node.upgrade().expect("Couldn't get starting node in setquery");
		let mut line = -(self.start.line as isize);
		let onscreen = |i: isize| i >= 0 && i < self.size.h as isize;
		while line < self.size.h as isize {
			if let Some(search) = cur.borrow().getsearch().as_ref() {
				for m in search.matchlines() {
					let matchline = line + m as isize;
					if onscreen(matchline) {
						to_redraw.insert(matchline as usize, Pos::new(Rc::downgrade(&cur), m));
					}
				}
			}
			cur.borrow_mut().search(&self.query);
			if self.query.is_some() {
				for m in cur.borrow().getsearch().as_ref().expect("Query is empty after calling search").matchlines() {
					let matchline = line + m as isize;
					if onscreen(matchline) {
						to_redraw.insert(matchline as usize, Pos::new(Rc::downgrade(&cur), m));
					}
				}
			}
			line += cur.borrow().lines() as isize;
			let next = Node::next(&cur).upgrade();
			match next {
				None => break,
				Some(n) => cur = n,
			}
		}
		for (line, pos) in to_redraw {
			self.drawline(line, pos);
		}
	}

	fn searchnext(&mut self, offset: isize) {
		if let Some(q) = &self.query {
			let sel = self.sel.upgrade().expect("Couldn't get selection in searchnext");
			let path = sel.borrow().searchfrom(q, offset * (if self.searchfwd { 1 } else { -1 }));
			let mut n = self.root.clone();
			let mut firstline: Option<isize> = None;
			for i in path {
				if n.borrow().expandable() && n.borrow().state != State::Expanded {
					let nextsib_pos = Pos::new(Node::nextsib(&n).clone(), 0);
					if firstline.is_none() {
						firstline = self.start.dist_fwd(nextsib_pos.clone()).map(|x| x as isize - 1);
					}
					Node::expand(&mut n, self.size.w);
					if n.borrow().is_before(sel.clone()) {
						if !n.borrow().is_before(self.start.node.upgrade().expect("Tree has invalid start position")) {
							// If n was before sel while collapsed, then n must have a next sibling
							let newlines = Pos::new(Rc::downgrade(&n), 0).dist_fwd(nextsib_pos.clone()).expect("Expanding node has no next sibling") - 1;
							self.offset += newlines as isize;
						}
					}
				}
				let target = n.borrow().children[i].clone();
				n = target;
			}
			let mut lastline = cmp::min(self.start.dist_fwd(Pos::nil()).expect("Couldn't find distance from start to end"), self.size.h) as isize;
			let scrolldist = self.select(n, true);
			if let Some(mut first) = firstline {
				if (scrolldist.abs() as usize) < self.size.h {
					first -= scrolldist;
					lastline -= scrolldist;
					if first < self.size.h as isize && lastline >= 0 {
						self.drawlines((cmp::max(first, 0) as usize, cmp::min(lastline as usize, self.size.h)));
					}
				}
			}
		}
	}

	fn search(&mut self, forward: bool) {
		if self.check_term_size() {
			let oldquery = self.query.clone();
			self.setquery(None);
			let incsearch = Box::new(|dt: &mut Tree, q: &str| dt.setquery(Self::query_from_str(q)));
			let size = self.size; // For borrowing
			let palette = self.palette.clone();
			let searchhist = self.searchhist.clone(); // Any way to avoid these expensive clones?
			// We should probably bubble up "non-internal" errors all the way up to the user, just to get nice error traces
			let res = ::prompt::prompt(self, (size.h, 0), size.w - 20, if forward { "/" } else { "?" }, "", searchhist, incsearch, &palette).expect("Prompt failed");
			if res == "" { self.setquery(oldquery); }
			else {
				self.searchhist.push(res);
				self.searchfwd = forward;
				let sel = self.sel.upgrade().expect("Couldn't get selection in search");
				if !sel.borrow().matches() { self.searchnext(1); }
			}
		}
	}

	fn setroot(&mut self, root: Rc<RefCell<Node<'a>>>) {
		self.root = root;
		self.sel = Rc::downgrade(&self.root);
		self.start = Pos::new(Rc::downgrade(&self.root), 0);
		self.offset = 0;
		self.accordion(&mut self.sel.upgrade().unwrap(), &|mut sel, w| Node::expand(&mut sel, w));
		self.select(self.first(), false);
		self.drawlines((0, self.size.h));
	}

	fn transform(&mut self, initq: &str) {
		if self.check_term_size() {
			let incxform = Box::new(|dt: &mut Tree, query: &str| {
				let root = match dt.source.propose(query, dt.size.w, dt.settings.hide_root) {
					Ok(tree) => Rc::clone(tree),
					Err(error) => {
						let message = error.iter().fold("Error:".to_string(), |acc, x| acc + "\n" + &x.to_string()).to_string();
						Rc::new(RefCell::new(Node::new_root(Box::new(StatMsg::new(message, 2)), dt.size.w, false)))
					},
				};
				dt.setroot(root);
			});
			let size = self.size; // For borrowing
			let palette = self.palette.clone();
			let xformhist = self.xformhist.clone();
			let res = ::prompt::prompt(self, (size.h, 0), size.w - 20, "|", initq, xformhist, incxform, &palette).expect("Prompt failed");
			if res == "" {
				let root = Rc::clone(self.source.reject());
				self.setroot(Rc::clone(&root));
			}
			else {
				self.source.accept();
				self.xformhist.push(res);
			}
		}
	}

	fn invokesel(&mut self) {
		let sel = self.sel.upgrade().expect("Couldn't get selection in invokesel");
		sel.borrow().invoke();
		self.redraw();
	}

	fn click(&mut self, y: usize) {
		let now = time::Instant::now();
		let oldsel = self.sel.clone();
		self.selpos(y);
		if oldsel.ptr_eq(&self.sel) && now.duration_since(self.lastclick).as_millis() < 400 {
			self.accordion(&mut self.sel.upgrade().unwrap(), &|mut sel, w| Node::toggle(&mut sel, w));
			self.lastclick = now.checked_sub(time::Duration::from_secs(60)).expect("We're less than 60 seconds after the epoch?"); // Epoch would be better
		}
		else { self.lastclick = now; }
	}

	fn mouse(&mut self, events: Vec<curses::MouseEvent>) {
		use curses::MouseClick::*;
		for event in events {
			match (event.button, event.kind) {
				(1, Press) => self.click(event.y as usize),
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
			false => {
				let numstr = self.numbuf.iter().collect::<String>();
				numstr.parse::<usize>().expect(&format!("Failed to parse numbuf \"{}\" as usize", numstr))
			},
		}
	}

	fn yanksel(&self) {
		use clipboard::{ClipboardProvider, ClipboardContext};
		let data = self.sel.upgrade().expect("Couldn't get selection in yanksel").borrow().yank();
		let maybe_clip: std::result::Result<ClipboardContext, Box<dyn std::error::Error>> = ClipboardProvider::new();
		// Swallowing an error getting the clipboard here isn't the best thing, but it's not the worst, and I'm not sure what the
		// better option is given the policy of no runtime errors during interactive session
		if let Ok(mut clip) = maybe_clip {
			let _ = clip.set_contents(data);
		}
	}

	fn seek(&self, rel: &dyn Fn(&Rc<RefCell<Node<'a>>>) -> Weak<RefCell<Node<'a>>>) -> Rc<RefCell<Node<'a>>> {
		let mut ret = self.sel.upgrade().expect("Couldn't get selection in seek");
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
		let done = Rc::new(RefCell::new(false));
		let d = done.clone();
		let mut keys: Keybinder<Self> = Keybinder::new();

		keys.register(&[&[ncurses::KEY_RESIZE]], Box::new(|dt, _| { dt.resize(); }));
		keys.register(&[&[ncurses::KEY_MOUSE]], Box::new(|dt, _| dt.mouse(curses::mouseevents())));
		keys.register(&[&ncstr(" ")], Box::new(|dt, _| dt.accordion(&mut dt.sel.upgrade().unwrap(), &|mut sel, w| Node::toggle(&mut sel, w))));
		keys.register(&[&ncstr("x")], Box::new(|dt, _| dt.accordion(&mut dt.sel.upgrade().unwrap(), &|mut sel, w| Node::recursive_expand(&mut sel, w))));
		keys.register(&[&[ncurses::KEY_RIGHT]], Box::new(|dt, _| dt.accordion(&mut dt.sel.upgrade().unwrap(), &|mut sel, w| Node::expand(&mut sel, w))));
		keys.register(&[&[ncurses::KEY_LEFT]], Box::new(|dt, _| dt.accordion(&mut dt.sel.upgrade().unwrap(), &|mut sel, _| Node::collapse(&mut sel))));
		keys.register(&[&ncstr("\n")], Box::new(|dt, _| dt.invokesel()));
		keys.register(&digits.iter().map(|x| &x[..]).collect::<Vec<&[i32]>>(), Box::new(|dt, digit| dt.addnum(digit[0] as u8 as char)));
		keys.register(&[&[0xc]], Box::new(|dt, _| dt.redraw())); // ^L
		keys.register(&[&ncstr("j"), &[ncurses::KEY_DOWN]], Box::new(|dt, _| { let sel = dt.seek(&|n: &Rc<RefCell<Node<'a>>>| Node::next(&n).clone()); dt.select(sel, true); }));
		keys.register(&[&ncstr("k"), &[ncurses::KEY_UP]], Box::new(|dt, _| { let sel = dt.seek(&|n: &Rc<RefCell<Node<'a>>>| Node::prev(&n).clone()); dt.select(sel, true); }));
		keys.register(&[&ncstr("J")], Box::new(|dt, _| { let sel = dt.seek(&|n: &Rc<RefCell<Node<'a>>>| Node::nextsib(&n).clone()); dt.select(sel, true); }));
		keys.register(&[&ncstr("K")], Box::new(|dt, _| { let sel = dt.seek(&|n: &Rc<RefCell<Node<'a>>>| Node::prevsib(&n).clone()); dt.select(sel, true); }));
		keys.register(&[&ncstr("p")], Box::new(|dt, _| { let sel = dt.seek(&|n: &Rc<RefCell<Node<'a>>>| Node::parent(&n).clone()); dt.select(sel, true); }));
		keys.register(&[&ncstr("g"), &[ncurses::KEY_HOME]], Box::new(|dt, _| { let sel = dt.first(); dt.select(sel, true); }));
		keys.register(&[&ncstr("G"), &[ncurses::KEY_END]], Box::new(|dt, _| { let sel = dt.last(); dt.select(sel, true); }));
		keys.register(&[&ncstr("H")], Box::new(|dt, _| { dt.selpos(0); }));
		keys.register(&[&ncstr("M")], Box::new(|dt, _| { let pos = dt.size.h / 2; dt.selpos(pos); }));
		keys.register(&[&ncstr("L")], Box::new(|dt, _| { let pos = dt.size.h - 1; dt.selpos(pos); }));
		keys.register(&[&[0x6], &[ncurses::KEY_NPAGE]], Box::new(|dt, _| { let dist = dt.getnum() * dt.size.h; dt.scroll(dist as isize); })); // ^F
		keys.register(&[&[0x2], &[ncurses::KEY_PPAGE]], Box::new(|dt, _| { let dist = dt.getnum() * dt.size.h; dt.scroll(-(dist as isize)); })); // ^B
		keys.register(&[&[0x4]], Box::new(|dt, _| { let dist = dt.getnum() * dt.size.h / 2; dt.scroll(dist as isize); })); // ^D
		keys.register(&[&[0x15]], Box::new(|dt, _| { let dist = dt.getnum() * dt.size.h / 2; dt.scroll(-(dist as isize)); })); // ^U
		keys.register(&[&[0x5]], Box::new(|dt, _| { dt.scroll(1); })); // ^E
		keys.register(&[&[0x19]], Box::new(|dt, _| { dt.scroll(-1); })); // ^Y
		keys.register(&[&ncstr("zz")], Box::new(|dt, _| { let dist = dt.offset - (dt.size.h as isize) / 2; dt.scroll(dist); }));
		keys.register(&[&ncstr("/")], Box::new(|dt, _| { dt.search(true); }));
		keys.register(&[&ncstr("?")], Box::new(|dt, _| { dt.search(false); }));
		keys.register(&[&ncstr("n")], Box::new(|dt, _| { let n = dt.getnum() as isize; dt.searchnext(n); }));
		keys.register(&[&ncstr("N")], Box::new(|dt, _| { let n = -(dt.getnum() as isize); dt.searchnext(n); }));
		keys.register(&[&ncstr("c")], Box::new(|dt, _| { dt.setquery(None); }));
		keys.register(&[&ncstr("|")], Box::new(|dt, _| { dt.transform(""); }));
		keys.register(&[&ncstr("C")], Box::new(|dt, _| { let root = Rc::clone(dt.source.clear()); dt.setroot(root); }));
		keys.register(&[&ncstr("r")], Box::new(|dt, _| { dt.refresh(&mut dt.sel.upgrade().expect("Couldn't get selection in refresh")); }));
		keys.register(&[&ncstr("R")], Box::new(|dt, _| { dt.refresh(&mut dt.root.clone()); dt.select(dt.first(), true); }));
		keys.register(&[&ncstr("y")], Box::new(|dt, _| { dt.yanksel(); }));
		keys.register(&[&ncstr("q")], Box::new(move |_, _| { *d.borrow_mut() = true; }));
		keys.register(&[&ncstr("-")], Box::new(|dt, _| {
			let mut node = (0..dt.getnum()).fold(dt.sel.upgrade().unwrap(), |n, _| Node::prev(&n).upgrade().unwrap_or(n));
			dt.accordion(&mut node, &|mut sel, w| Node::toggle(&mut sel, w));
		}));
		keys.register(&[&ncstr("=")], Box::new(|dt, _| {
			let mut node = (0..dt.getnum()).fold(dt.sel.upgrade().unwrap(), |n, _| Node::next(&n).upgrade().unwrap_or(n));
			dt.accordion(&mut node, &|mut sel, w| Node::toggle(&mut sel, w));
		}));

		self.resize();
		self.accordion(&mut self.sel.upgrade().unwrap(), &|mut sel, w| Node::expand(&mut sel, w));
		self.select(self.first(), false);
		while !*done.borrow() {
			let (maybe_action, cmd) = keys.wait(self);
			if let Some(action) = maybe_action {
				let lambda: &mut dyn FnMut(&mut Self, &[i32]) = &mut *action.borrow_mut();
				let lock = Arc::clone(&self.lock);
				let _guard = lock.lock();
				lambda(self, &cmd);
			}
			if !digits.contains(&cmd) { self.clearnum(); }
		}
	}
}

unsafe impl<'a> Sync for Tree<'a> { }
