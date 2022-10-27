use std::cmp;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};
use std::time;
use ::curses;
use ::interface::*;
use ::keybinder::Keybinder;
use ::owning_ref::OwningHandle;
use ::regex::Regex;
use super::node::{Node, State};
use super::pos::Pos;
use super::statmsg::StatMsg;
use anyhow::Result;

type OwnedRoot<'a> = OwningHandle<Box<dyn Source>, Box<Arc<Mutex<Node<'a>>>>>;

struct TransformManager<'a> {
	base: OwnedRoot<'a>,
	cur: Option<OwnedRoot<'a>>,
	next: Option<OwnedRoot<'a>>,
}

impl<'a> TransformManager<'a> {
	fn new_owned_root(source: Box<dyn Source>, w: usize, hideroot: bool) -> OwnedRoot<'a> {
		OwningHandle::new_with_fn(source, |s| unsafe { Box::new(Arc::new(Mutex::new(Node::new_root(s.as_ref().expect("OwningHandle provided null pointer").root(), w, hideroot)))) } )
	}

	pub fn new(source: Box<dyn Source>, w: usize, hideroot: bool) -> Self {
		Self {
			base: Self::new_owned_root(source, w, hideroot),
			cur: None,
			next: None,
		}
	}

	pub fn clear(&mut self) -> &Arc<Mutex<Node<'a>>> {
		self.next = None;
		self.cur = None;
		&*self.base
	}

	pub fn propose(&mut self, q: &str, w: usize, hideroot: bool) -> Result<&Arc<Mutex<Node<'a>>>> {
		match self.cur.as_ref().unwrap_or(&self.base).as_owner().transform(q) {
			Ok(tree) => {
				self.next = Some(Self::new_owned_root(tree, w, hideroot));
				Ok(&*(self.next.as_ref().expect("self.next was not Some after assigning")))
			},
			Err(error) => Err(error),
		}
	}

	pub fn accept(&mut self) {
		std::mem::swap(&mut self.cur, &mut self.next);
		self.next = None;
	}

	pub fn reject(&mut self) -> &Arc<Mutex<Node<'a>>> {
		self.next = None;
		&*(self.cur.as_ref().unwrap_or(&self.base))
	}
}

pub struct Tree<'a> {
	source: TransformManager<'a>, // Holds tree source and manages transformations
	root: Arc<Mutex<Node<'a>>>, // Root node of the displayed tree
	sel: Weak<Mutex<Node<'a>>>, // Currently selected node
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
	quit: Arc<Mutex<bool>>, // Whether we should quit after next update
	msg: String, // Current message to desplay in the status bar
	lock: Arc<Mutex<()>>, // Single-thread all updates
}

impl<'a> Tree<'a> {
	pub fn new(tree: Box<dyn Source>, colors: Vec<Color>, settings: Settings) -> Result<Self> {
		let size = curses::scrsize();
		let mut source = TransformManager::new(tree, size.w, settings.hide_root);
		let root = Arc::clone(source.clear());
		let mut fgcol = super::FG_COLORS.to_vec();
		fgcol.extend(colors);
		let palette = curses::Palette::new(fgcol, super::BG_COLORS.to_vec())?;
		Ok(Tree {
			source: source,
			sel: Arc::downgrade(&root),
			size: size,
			start: Pos::new(Arc::downgrade(&root), 0),
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
			quit: Arc::new(Mutex::new(false)),
			msg: String::new(),
			lock: Arc::new(Mutex::new(())),
		})
	}

	fn first(&self) -> Arc<Mutex<Node<'a>>> {
		let mut cur = self.root.clone();
		while cur.lock().expect("Poisoned lock").lines() == 0 {
			if let Some(next) = Node::next(&cur).upgrade() { cur = next; }
			else { return self.root.clone(); }
		}
		cur
	}

	fn last(&self) -> Arc<Mutex<Node<'a>>> {
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
		const DEBUG: bool = false;
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
				node.lock().expect("Poisoned lock").search(&self.query);
				node.lock().expect("Poisoned lock").drawline(&self.palette, cur.line, selected);
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
		let lines = sel.lock().expect("Poisoned lock").lines();
		//assert!(self.offset + lines as isize >= 0 && self.offset < self.size.h as isize);
		(cmp::max(self.offset, 0) as usize, cmp::min((self.offset + lines as isize) as usize, self.size.h))
	}

	fn echo(&mut self, s: String) {
		self.msg = s;
	}

	fn statline(&self) {
		if self.check_term_size() {
			ncurses::mv(self.size.h as i32, 0);
			ncurses::clrtoeol();
			ncurses::addstr(&self.msg); // TODO Truncate to fit
			ncurses::mv(self.size.h as i32, self.size.w as i32 - 8);
			ncurses::addstr(&self.numbuf.iter().collect::<String>());
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
					let lines = sel.lock().expect("Poisoned lock").lines() as isize;
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
					self.offset -= newsel.lock().expect("Poisoned lock").lines() as isize;
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

	fn select(&mut self, sel: Arc<Mutex<Node<'a>>>, scrollin: bool) -> isize {
		if self.check_term_size() {
			let oldsel = self.sel.upgrade().expect("Couldn't get selection in select");
			let same = Arc::ptr_eq(&oldsel, &sel);
			let down = Node::is_before(&oldsel, &sel);
			let oldlines = self.sellines();
			self.offset += match down {
				true => Pos::new(self.sel.clone(), 0).dist_fwd(Pos::new(Arc::downgrade(&sel), 0))
					.expect("Down is true but new selection not after old") as isize,
				false => -(Pos::new(Arc::downgrade(&sel), 0).dist_fwd(Pos::new(self.sel.clone(), 0))
					.expect("Down is false but new selection not before old") as isize),
			};
			self.sel = Arc::downgrade(&sel);
			let scrolldist = self.scroll({
				let lines = sel.lock().expect("Poisoned lock").lines() as isize;
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
		let mut cur = Arc::downgrade(&self.root);
		while let Some(n) = cur.upgrade() {
			f(&mut n.lock().expect("Poisoned lock"));
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
		if size.h < 1 { size.h = 1; }
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

	fn accordion(&mut self, mut node: &mut Arc<Mutex<Node<'a>>>, op: &dyn Fn(&mut Arc<Mutex<Node>>, usize) -> ()) {
		let start = self.start.node.upgrade().expect("Couldn't get start node in accordion");
		let sel = self.sel.upgrade().expect("Couldn't get selection in accordion");
		if Node::is_before(&node, &sel) {
			if Node::is_before(&node, &start) && !Node::is_ancestor_of(&node, &start) { op(&mut node, self.size.w); }
			else {
				if Node::is_ancestor_of(&node, &sel) {
					self.select(node.clone(), true); // TODO Use path resolution to select a new sel
					op(&mut node, self.size.w);
				}
				else {
					let oldoff = Pos::new(Arc::downgrade(&node), 0).dist_fwd(Pos::new(Arc::downgrade(&sel), 0)).expect("is_before returned true, but dist_fwd returned None") as isize;
					op(&mut node, self.size.w);
					let newoff = Pos::new(Arc::downgrade(&node), 0).dist_fwd(Pos::new(Arc::downgrade(&sel), 0)).expect("is_before returned true, but dist_fwd returned None") as isize;
					if Node::is_before(&node, &start) {
						// The node is an ancestor of the start node.  In the case of a collapse,
						// it is possible that so much is collased that the entire document gets
						// scrolled off the top of the screen and the start node is no longer
						// valid, so we can't just scroll like we do in the other case.
						self.start = Pos::new(Arc::downgrade(&sel), 0).seek(-self.offset, true);
						self.offset = self.start.dist_fwd(Pos::new(self.sel.clone(), 0)).expect("start is not before sel") as isize;
					}
					else {
						let diff = newoff - oldoff;
						self.offset += diff;
						self.scroll(diff);
					}
				}
				self.redraw();
			}
		}
		else if Node::is_before(&self.start.fwd(self.size.h - 1, true).node.upgrade().expect("Safe traversal returned None"), &node) { op(&mut node, self.size.w); }
		else {
			let mut maxend = Pos::new(Arc::downgrade(&node), 0).dist_fwd(Pos::nil()).expect("Failed to find distance to end of document");
			op(&mut node, self.size.w);
			maxend = cmp::max(maxend, Pos::new(Arc::downgrade(&node), 0).dist_fwd(Pos::nil()).expect("Failed to find distance to end of document"));
			// Unfortunately we need to redraw the whole selection, because we don't know how much it's changed due to the (un)expansion
			let startoff = cmp::max(self.offset, 0) as usize;
			self.drawlines((startoff, cmp::min(self.size.h, startoff + maxend + 1)));
		}
	}

	fn refresh(&mut self, node: &mut Arc<Mutex<Node<'a>>>) {
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
			if let Some(search) = cur.lock().expect("Poisoned lock").getsearch().as_ref() {
				for m in search.matchlines() {
					let matchline = line + m as isize;
					if onscreen(matchline) {
						to_redraw.insert(matchline as usize, Pos::new(Arc::downgrade(&cur), m));
					}
				}
			}
			cur.lock().expect("Poisoned lock").search(&self.query);
			if self.query.is_some() {
				for m in cur.lock().expect("Poisoned lock").getsearch().as_ref().expect("Query is empty after calling search").matchlines() {
					let matchline = line + m as isize;
					if onscreen(matchline) {
						to_redraw.insert(matchline as usize, Pos::new(Arc::downgrade(&cur), m));
					}
				}
			}
			line += cur.lock().expect("Poisoned lock").lines() as isize;
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
			let path = Node::searchfrom(&sel, q, offset * (if self.searchfwd { 1 } else { -1 }));
			let mut n = self.root.clone();
			let mut firstline: Option<isize> = None;
			for i in path {
				let (expandable, state) = {
					let locked = n.lock().expect("Poisoned lock");
					(locked.expandable(), locked.state)
				};
				if expandable && state != State::Expanded {
					let nextsib_pos = Pos::new(Node::nextsib(&n).clone(), 0);
					if firstline.is_none() {
						firstline = self.start.dist_fwd(nextsib_pos.clone()).map(|x| x as isize - 1);
					}
					Node::expand(&mut n, self.size.w);
					if Node::is_before(&n, &sel) {
						if !Node::is_before(&n, &self.start.node.upgrade().expect("Tree has invalid start position")) {
							// If n was before sel while collapsed, then n must have a next sibling
							let newlines = Pos::new(Arc::downgrade(&n), 0).dist_fwd(nextsib_pos.clone()).expect("Expanding node has no next sibling") - 1;
							self.offset += newlines as isize;
						}
					}
				}
				let target = n.lock().expect("Poisoned lock").children[i].clone();
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
				if !sel.lock().expect("Poisoned lock").matches() { self.searchnext(1); }
			}
		}
	}

	fn setroot(&mut self, root: Arc<Mutex<Node<'a>>>) {
		self.root = root;
		self.sel = Arc::downgrade(&self.root);
		self.start = Pos::new(Arc::downgrade(&self.root), 0);
		self.offset = 0;
		self.accordion(&mut self.sel.upgrade().expect("Couldn't get selection in setroot"), &|mut sel, w| Node::expand(&mut sel, w));
		self.select(self.first(), false);
		self.drawlines((0, self.size.h));
	}

	fn transform(&mut self, initq: &str) {
		if self.check_term_size() {
			let incxform = Box::new(|dt: &mut Tree, query: &str| {
				let root = match dt.source.propose(query, dt.size.w, dt.settings.hide_root) {
					Ok(tree) => Arc::clone(tree),
					Err(error) => {
						let message = error.chain().fold("Error:".to_string(), |acc, x| acc + "\n" + &x.to_string()).to_string();
						Arc::new(Mutex::new(Node::new_root(Box::new(StatMsg::new(message, 2)), dt.size.w, false)))
					},
				};
				dt.setroot(root);
			});
			let size = self.size; // For borrowing
			let palette = self.palette.clone();
			let xformhist = self.xformhist.clone();
			let res = ::prompt::prompt(self, (size.h, 0), size.w - 20, "|", initq, xformhist, incxform, &palette).expect("Prompt failed");
			if res == "" {
				let root = Arc::clone(self.source.reject());
				self.setroot(Arc::clone(&root));
			}
			else {
				self.source.accept();
				self.xformhist.push(res);
			}
		}
	}

	fn invokesel(&mut self) {
		let sel = self.sel.upgrade().expect("Couldn't get selection in invokesel");
		sel.lock().expect("Poisoned lock").invoke();
		self.redraw();
	}

	fn click(&mut self, y: usize) {
		let now = time::Instant::now();
		let oldsel = self.sel.clone();
		self.selpos(y);
		if oldsel.ptr_eq(&self.sel) && now.duration_since(self.lastclick).as_millis() < 400 {
			self.accordion(&mut self.sel.upgrade().expect("Couldn't get selection in click"), &|mut sel, w| Node::toggle(&mut sel, w));
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
		// Swallowing an error getting the clipboard here isn't the best thing, but it's not the worst, and I'm not sure what the
		// better option is given the policy of no runtime errors during interactive session
		if let Ok(mut clip) = arboard::Clipboard::new() {
			let data = self.sel.upgrade().expect("Couldn't get selection in yanksel").lock().expect("Poisoned lock").yank();
			let _ = clip.set_text(data);
		}
	}

	fn seek(&self, rel: &dyn Fn(&Arc<Mutex<Node<'a>>>) -> Weak<Mutex<Node<'a>>>) -> Arc<Mutex<Node<'a>>> {
		let mut ret = self.sel.upgrade().expect("Couldn't get selection in seek");
		for _ in 1..=self.getnum() {
			let next = rel(&ret);
			if let Some(newret) = next.upgrade() { ret = newret; }
			else { break; }
		}
		ret
	}

	fn command(&mut self, cmd: &[&str]) -> Result<()> {
		match &cmd[..] {
			&["select", dir] => match dir {
				"prev" => { let sel = self.seek(&|n: &Arc<Mutex<Node<'a>>>| Node::prev(&n).clone()); self.select(sel, true); },
				"next" => { let sel = self.seek(&|n: &Arc<Mutex<Node<'a>>>| Node::next(&n).clone()); self.select(sel, true); },
				"prevsib" => { let sel = self.seek(&|n: &Arc<Mutex<Node<'a>>>| Node::prevsib(&n).clone()); self.select(sel, true); },
				"nextsib" => { let sel = self.seek(&|n: &Arc<Mutex<Node<'a>>>| Node::nextsib(&n).clone()); self.select(sel, true); },
				"parent" => { let sel = self.seek(&|n: &Arc<Mutex<Node<'a>>>| Node::parent(&n).clone()); self.select(sel, true); },
				"first" => { let sel = self.first(); self.select(sel, true); },
				"last" => { let sel = self.last(); self.select(sel, true); },
				"top" => { self.selpos(0); },
				"middle" => { let pos = self.size.h / 2; self.selpos(pos); },
				"bottom" => { let pos = self.size.h - 1; self.selpos(pos); },
				_ => bail!("Unknown direction"),
			},
			&["scroll", dir] => match dir {
				"up" => { self.scroll(-1); },
				"down" => { self.scroll(1); },
				"center" => { let dist = self.offset - (self.size.h as isize) / 2; self.scroll(dist); },
				_ => bail!("Unknown direction"),
			},
			&["scroll", dir, frac] => {
				let numfrac = frac.parse::<usize>().unwrap();
				match dir {
					"up" => { let dist = self.getnum() * self.size.h * numfrac / 100; self.scroll(-(dist as isize)); },
					"down" => { let dist = self.getnum() * self.size.h * numfrac / 100; self.scroll(dist as isize); },
					_ => bail!("Unknown direction"),
				};
			},
			&["node", act] => match act {
				"expand" => { self.accordion(&mut self.sel.upgrade().expect("Couldn't get selection"), &|mut sel, w| Node::expand(&mut sel, w)) },
				"recursive-expand" => { self.accordion(&mut self.sel.upgrade().expect("Couldn't get selection"), &|mut sel, w| Node::recursive_expand(&mut sel, w)) },
				"collapse" => { self.accordion(&mut self.sel.upgrade().expect("Couldn't get selection"), &|mut sel, _| Node::collapse(&mut sel)) },
				"toggle" => { self.accordion(&mut self.sel.upgrade().expect("Couldn't get selection"), &|mut sel, w| Node::toggle(&mut sel, w)) },
				_ => bail!("Unknown action"),
			},
			&["search", act] => match act {
				"forward" => { self.search(true); },
				"backward" => { self.search(false); },
				"next" => { let n = self.getnum() as isize; self.searchnext(n); },
				"prev" => { let n = -(self.getnum() as isize); self.searchnext(n); },
				"clear" => { self.setquery(None); },
				_ => bail!("Unknown action"),
			}
			&["transform"] => { self.transform(""); },
			&["transform", "reset"] => { let root = Arc::clone(self.source.clear()); self.setroot(root); },
			&["invoke"] => { self.invokesel(); },
			&["yank"] => { self.yanksel(); },
			&["refresh", node] => match node {
				"root" => { self.refresh(&mut self.root.clone()); self.select(self.first(), true); },
				"current" => { self.refresh(&mut self.sel.upgrade().expect("Couldn't get selection in refresh")); },
				_ => bail!("Unknown node"),
			},
			&["redraw"] => { self.redraw(); },
			&["command"] => { self.cmdline(); },
			&["echo", ref args @ ..] => { self.echo(args.join(" ")); },
			&["q"] => { *self.quit.lock().expect("Poisoned lock") = true; },
			&["quit"] => { *self.quit.lock().expect("Poisoned lock") = true; },
			&["nop"] => { },
			&[] => { },
			_ => bail!("Unknown command"),
		}
		Ok(())
	}

	fn cmdline(&mut self) {
		let inccb = Box::new(|_: &mut Tree, _: &str| { });
		let palette = self.palette.clone();
		let res = ::prompt::prompt(self, (self.size.h, 0), self.size.w - 20, ":", "", vec![], inccb, &palette).expect("Prompt failed");
		if res != "" {
			// Someday, we may want to replace this with "real" parsing with Nom.  In that case, be
			// sure to replace the `cmd.split()` in `interactive()` below as well.
			let tokens = Regex::new("\\s+").expect("Invalid internal regex").split(&res).collect::<Vec<&str>>();
			if let Err(e) = self.command(&tokens) {
				self.echo(e.to_string());
			}
		}
	}

	pub fn interactive(&mut self) {
		let digits = ('0'..='9').map(|x| vec![x as i32]).collect::<Vec<Vec<i32>>>();
		let mut keys: Keybinder<Self> = Keybinder::new();
		let keymap = HashMap::from([
			("j", "select next"),
			("Down", "select next"),
			("J", "select nextsib"),
			("k", "select prev"),
			("Up", "select prev"),
			("K", "select prevsib"),
			("p", "select parent"),
			("g", "select first"),
			("G", "select last"),
			("H", "select top"),
			("M", "select middle"),
			("L", "select bottom"),
			("\\ ", "node toggle"),
			("Left", "node collapse"),
			("Right", "node expand"),
			("x", "node recursive-expand"),
			("^F", "scroll down 100"),
			("Next", "scroll down 100"),
			("^B", "scroll up 100"),
			("Prior", "scroll up 100"),
			("^D", "scroll down 50"),
			("^U", "scroll up 50"),
			("^E", "scroll down"),
			("^Y", "scroll up"),
			("z z", "scroll center"),
			("/", "search forward"),
			("?", "search backward"),
			("n", "search next"),
			("N", "search prev"),
			("c", "search clear"),
			("|", "transform"),
			("C", "transform reset"),
			("r", "refresh current"),
			("R", "refresh root"),
			("y", "yank"),
			("\n", "invoke"),
			("^L", "redraw"),
			(":", "command"),
			("q", "quit"),
		]);
		for (key, cmd) in keymap {
			let cmdparts = cmd.split(' ').collect::<Vec<&str>>();
			match curses::parse_keysyms(key) {
				Ok(keyseq) => { keys.register(&[&keyseq], Box::new(move |dt, _| { if let Err(e) = dt.command(&cmdparts) { dt.echo(e.to_string()); } })); },
				Err(e) => self.echo(e.to_string()),
			}
		}
		keys.register(&digits.iter().map(|x| &x[..]).collect::<Vec<&[i32]>>(), Box::new(|dt, digit| dt.addnum(digit[0] as u8 as char)));
		keys.register(&[&[ncurses::KEY_RESIZE]], Box::new(|dt, _| { dt.resize(); }));
		keys.register(&[&[ncurses::KEY_MOUSE]], Box::new(|dt, _| dt.mouse(curses::mouseevents())));

		self.resize();
		self.accordion(&mut self.sel.upgrade().expect("Couldn't get selection"), &|mut sel, w| Node::expand(&mut sel, w));
		self.select(self.first(), false);
		while !*self.quit.lock().expect("Poisoned lock") {
			let (maybe_action, cmd) = keys.wait(self);
			if let Some(action) = maybe_action {
				let lambda: &mut dyn FnMut(&mut Self, &[i32]) = &mut *action.borrow_mut();
				let lock = Arc::clone(&self.lock);
				let _guard = lock.lock().expect("Poisoned lock");
				lambda(self, &cmd);
			}
			if !digits.contains(&cmd) { self.numbuf.clear(); }
			self.statline();
			self.msg.clear();
		}
	}
}

unsafe impl<'a> Sync for Tree<'a> { }
