use std::rc::{Rc, Weak};
use std::cell::RefCell;
use std::time;
use std::collections::HashMap;
use ::regex::Regex;
use ::curses;
use ::interface::{Value, Color};
use ::keybinder::Keybinder;
use super::node::Node;
use super::pos::Pos;
use ::errors::*;

pub struct Tree<'a> {
	root: Rc<RefCell<Node<'a>>>,
	sel: Weak<RefCell<Node<'a>>>,
	size: curses::Size,
	start: Pos<'a>,
	offset: isize,
	down: bool,
	query: Option<Regex>,
	searchhist: Vec<String>,
	searchfwd: bool,
	lastclick: time::Instant,
	numbuf: Vec<char>,
	palette: curses::Palette,
}

impl<'a> Tree<'a> {
	pub fn new(json: Box<dyn Value<'a> + 'a>, colors: Vec<Color>) -> Result<Self> {
		let size = curses::scrsize();
		let root = Rc::new(RefCell::new(Node::new_root(json, size.w)));
		let mut fgcol = super::FG_COLORS.to_vec();
		fgcol.extend(colors);
		let palette = curses::Palette::new(fgcol, super::BG_COLORS.to_vec())?;
		Ok(Tree {
			sel: Rc::downgrade(&root),
			size: size,
			start: Pos::new(Rc::downgrade(&root), 0),
			offset: 0,
			down: true,
			query: None,
			searchhist: vec![],
			searchfwd: true,
			lastclick: time::Instant::now().checked_sub(time::Duration::from_secs(60)).expect("This program cannot be run before January 2, 1970"),
			numbuf: vec![],
			palette: palette,
			root: root,
		})
	}

	fn last(&self) -> Rc<RefCell<Node<'a>>> {
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
		let offset = std::cmp::max(self.offset, 0) as usize;
		let sel = self.sel.upgrade().expect("Couldn't get selection in sellines");
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
					i if i > 0 => self.start.dist_fwd(newstart.clone()).expect("Seek returned an incorrect node") as isize,
					i if i < 0 => -(newstart.dist_fwd(self.start.clone()).expect("Seek returned an incorrect nodee") as isize),
					_ => 0,
				};
				let dist = diff.abs() as usize;
				self.start = newstart;
				self.offset -= diff;
				if by > 0 {
					while self.offset < 0 {
						match self.down {
							false => {
								let sel = self.sel.upgrade().expect("Couldn't get selection in scroll");
								self.offset += (sel.borrow().lines() - 1) as isize;
								self.down = true
							}
							true => {
								let sel1 = self.sel.upgrade().expect("Couldn't get selection in scroll");
								self.sel = sel1.borrow().next.clone();
								let sel2 = self.sel.upgrade().expect("Couldn't get selection in scroll");
								self.offset += sel2.borrow().lines() as isize;
							}
						}
					}
				}
				else {
					while self.offset >= self.size.h as isize {
						match self.down {
							true => {
								let sel = self.sel.upgrade().expect("Couldn't get selection in scroll");
								self.offset -= (sel.borrow().lines() - 1) as isize;
								self.down = false;
							}
							false => {
								let sel1 = self.sel.upgrade().expect("Couldn't get selection in scroll");
								self.sel = sel1.borrow().prev.clone();
								let sel2 = self.sel.upgrade().expect("Couldn't get selection in scroll");
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
					if !self.sel.ptr_eq(&oldsel) { self.drawlines(self.sellines()); }
				}
				self.statline();
				diff
			},
		}
	}

	fn select(&mut self, sel: Rc<RefCell<Node<'a>>>) -> isize {
		if self.check_term_size() {
			let oldsel = self.sel.upgrade().expect("Couldn't get selection in select");
			let same = Rc::ptr_eq(&oldsel, &sel);
			let down = oldsel.borrow().is_before(sel.clone());
			let oldlines = self.sellines();
			let curpos = match self.down {
				true => oldsel.borrow().lines() - 1,
				false => 0,
			};
			match down {
				true => self.offset += Pos::new(self.sel.clone(), curpos).dist_fwd(Pos::new(Rc::downgrade(&sel), sel.borrow().lines() - 1))
					.expect("Down is true but new selection not after old") as isize,
				false => self.offset -= Pos::new(Rc::downgrade(&sel), 0).dist_fwd(Pos::new(self.sel.clone(), curpos))
					.expect("Down is false but new selection not before old") as isize,
			};
			self.down = down;
			self.sel = Rc::downgrade(&sel);
			let mut scrolldist = 0;
			if self.offset < 0 { let sd = self.offset; scrolldist = self.scroll(sd); }
			else if self.offset >= self.size.h as isize { let sd = self.offset - self.size.h as isize + 1; scrolldist = self.scroll(sd); }
			else { self.statline(); }
			if oldlines.0 as isize - scrolldist < self.size.h as isize && oldlines.1 as isize - scrolldist >= 0 && !same {
				self.drawlines(( // Clear the old selection
					std::cmp::max(oldlines.0 as isize - scrolldist, 0) as usize,
					std::cmp::min(oldlines.1 as isize - scrolldist, self.size.h as isize) as usize
				));
			}
			if (scrolldist.abs() as usize) < self.size.h {
				let mut sellines = self.sellines();
				if scrolldist > 0 {
					sellines = (std::cmp::min(sellines.0, self.size.h - scrolldist as usize), std::cmp::min(sellines.1, self.size.h - scrolldist as usize));
				}
				else if scrolldist < 0 {
					sellines = (std::cmp::max(sellines.0, -scrolldist as usize), std::cmp::max(sellines.1, -scrolldist as usize));
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
			cur = n.borrow().next.clone();
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
			let line = match self.down {
				false => 0,
				true => sel.borrow().lines() - 1,
			};
			// If `start` is the last line of a multi-line wrapped node, but we make the terminal
			// wider and the node unwraps to fewer lines, `sel` will now be before `start`.
			let curpos = Pos::new(self.sel.clone(), line);
			self.offset = {
				let fwd = self.start.dist_fwd(curpos.clone()).map(|x| x as isize);
				if let Some(ret) = fwd { ret }
				else { -(curpos.dist_fwd(self.start.clone()).expect("Could not determine new offset in resize") as isize) }
			};
			// TODO If start.node == sel and sel is multi-line, then on each resize we'll jump to
			// the top of sel, which is not terrible but perhaps not desirable
			self.select(sel);
			self.redraw();
		}
	}

	fn selpos(&mut self, line: usize) {
		let target = self.start.fwd(line, true).node.upgrade().expect("Tried to select invalid line");
		self.select(target);
	}

	fn accordion(&mut self, op: &dyn Fn(&mut Rc<RefCell<Node>>, usize) -> ()) {
		ncurses::mvaddstr(self.size.h as i32, 0, "Loading...");
		ncurses::refresh();
		let mut sel = self.sel.upgrade().expect("Couldn't get selection in accordion");
		let lines_before = sel.borrow().lines() as isize;
		let mut maxend = Pos::new(Rc::downgrade(&sel), 0).dist_fwd(Pos::nil()).expect("Failed to find distance to end of document");
		let w = self.size.w;
		op(&mut sel, w);
		let lines_after = sel.borrow().lines() as isize;
		maxend = std::cmp::max(maxend, Pos::new(Rc::downgrade(&sel), 0).dist_fwd(Pos::nil()).expect("Failed to find distance to end of document"));
		if self.down { self.offset += lines_after - lines_before; }
		let drawstart = match self.down {
			true => self.offset - lines_after + 1,
			false => self.offset,
		};
		// Unfortunately we need to redraw the whole selection, because we don't know how much it's changed due to the (un)expansion
		self.drawlines((drawstart as usize, std::cmp::min(self.size.h, self.offset as usize + maxend)));
		self.statline();
	}

	fn refresh(&mut self, node: &mut Rc<RefCell<Node<'a>>>) {
		// We can try doing fancier things down the line, but for now select the node being
		// refreshed to avoid dealing with a selection that no longer exists
		self.select(node.clone());
		self.accordion(&|node, w| Node::refresh(node, w));
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
			let next = cur.borrow().next.upgrade();
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
				if n.borrow().expandable() && !n.borrow().expanded {
					let nextsib_pos = Pos::new(n.borrow().nextsib.clone(), 0);
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
			let mut lastline = std::cmp::min(self.start.dist_fwd(Pos::nil()).expect("Couldn't find distance from start to end"), self.size.h) as isize;
			let scrolldist = self.select(n);
			if let Some(mut first) = firstline {
				if (scrolldist.abs() as usize) < self.size.h {
					first -= scrolldist;
					lastline -= scrolldist;
					if first < self.size.h as isize && lastline >= 0 {
						self.drawlines((std::cmp::max(first, 0) as usize, std::cmp::min(lastline as usize, self.size.h)));
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
			if res == "" {
				self.setquery(oldquery);
			}
			else {
				self.searchhist.push(res);
				self.searchfwd = forward;
				self.searchnext(1);
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
			self.accordion(&|mut sel, w| Node::toggle(&mut sel, w));
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
		keys.register(&[&ncstr(" ")], Box::new(|dt, _| dt.accordion(&|mut sel, w| Node::toggle(&mut sel, w))));
		keys.register(&[&ncstr("x")], Box::new(|dt, _| dt.accordion(&|mut sel, w| Node::recursive_expand(&mut sel, w))));
		keys.register(&[&[ncurses::KEY_RIGHT]], Box::new(|dt, _| dt.accordion(&|mut sel, w| Node::expand(&mut sel, w))));
		keys.register(&[&[ncurses::KEY_LEFT]], Box::new(|dt, _| dt.accordion(&|mut sel, _| Node::collapse(&mut sel))));
		keys.register(&[&ncstr("\n")], Box::new(|dt, _| dt.invokesel()));
		keys.register(&digits.iter().map(|x| &x[..]).collect::<Vec<&[i32]>>(), Box::new(|dt, digit| dt.addnum(digit[0] as u8 as char)));
		keys.register(&[&[0xc]], Box::new(|dt, _| dt.redraw())); // ^L
		keys.register(&[&ncstr("j"), &[ncurses::KEY_DOWN]], Box::new(|dt, _| { let sel = dt.seek(&|n: &Rc<RefCell<Node<'a>>>| n.borrow().next.clone()); dt.select(sel); }));
		keys.register(&[&ncstr("k"), &[ncurses::KEY_UP]], Box::new(|dt, _| { let sel = dt.seek(&|n: &Rc<RefCell<Node<'a>>>| n.borrow().prev.clone()); dt.select(sel); }));
		keys.register(&[&ncstr("J")], Box::new(|dt, _| { let sel = dt.seek(&|n: &Rc<RefCell<Node<'a>>>| n.borrow().nextsib.clone()); dt.select(sel); }));
		keys.register(&[&ncstr("K")], Box::new(|dt, _| { let sel = dt.seek(&|n: &Rc<RefCell<Node<'a>>>| n.borrow().prevsib.clone()); dt.select(sel); }));
		keys.register(&[&ncstr("p")], Box::new(|dt, _| { let sel = dt.seek(&|n: &Rc<RefCell<Node<'a>>>| n.borrow().parent.clone()); dt.select(sel); }));
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
		keys.register(&[&ncstr("c")], Box::new(|dt, _| { dt.setquery(None); }));
		keys.register(&[&ncstr("r")], Box::new(|dt, _| { dt.refresh(&mut dt.sel.upgrade().expect("Couldn't get selection in refresh")); }));
		keys.register(&[&ncstr("R")], Box::new(|dt, _| { dt.refresh(&mut dt.root.clone()); }));
		keys.register(&[&ncstr("y")], Box::new(|dt, _| { dt.yanksel(); }));
		keys.register(&[&ncstr("q")], Box::new(move |_, _| { *d.borrow_mut() = true; }));

		self.resize();
		self.accordion(&|mut sel, w| Node::toggle(&mut sel, w));
		while !*done.borrow() {
			let cmd = keys.wait(self);
			if !digits.contains(&cmd) { self.clearnum(); }
		}
	}
}
