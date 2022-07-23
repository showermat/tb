use std::sync::{Arc, Mutex, Weak};
use ::regex::Regex;
use ::format::{Preformatted, Search};
use ::curses;
use super::value::Value;
use ::interface::Value as BackendValue;
use super::COLWIDTH;
use super::statmsg::StatMsg;

struct NodeCache {
	prefix0: String,
	prefix1: String,
	placeholder: Preformatted,
	content: Preformatted,
	search: Option<Search>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum State {
	Collapsed,
	Loading,
	Expanded,
}

pub struct Node<'a> {
	pub children: Vec<Arc<Mutex<Node<'a>>>>,
	parent: Weak<Mutex<Node<'a>>>,
	prev: Weak<Mutex<Node<'a>>>,
	next: Weak<Mutex<Node<'a>>>,
	prevsib: Weak<Mutex<Node<'a>>>,
	nextsib: Weak<Mutex<Node<'a>>>,
	pub state: State,
	last: bool,
	value: Arc<Mutex<Value<'a>>>,
	cache: NodeCache,
	hide: bool,
}

impl<'a> Node<'a> {
	pub fn depth(&self) -> usize {
		match self.parent.upgrade() {
			None => 0,
			Some(p) if p.lock().expect("Poisoned lock").hide => p.lock().expect("Poisoned lock").depth(),
			Some(p) => p.lock().expect("Poisoned lock").depth() + 1,
		}
	}

	pub fn lines(&self) -> usize {
		if self.hide { 0 }
		else {
			match self.state {
				State::Loading | State::Expanded => self.cache.placeholder.len(),
				State::Collapsed => self.cache.content.len(),
			}
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
	/* NOTE This function does not work for general-case insertion!  It is only designed for
	 * inserting children and siblings into the tree, not parents.  For our purposes, that is
	 * sufficient.  If the node on one or both sides is deeper in the tree than the one being
	 * added, sibling links will not be updated correctly.
	 */
	fn insert(after: &mut Arc<Mutex<Node<'a>>>, node: &mut Arc<Mutex<Node<'a>>>) {
		let mut borrowed_node = node.lock().expect("Poisoned lock");
		let mut borrowed_after = after.lock().expect("Poisoned lock");
		if let Some(next) = borrowed_after.next.upgrade() {
			let mut borrowed_next = next.lock().expect("Poisoned lock");
			borrowed_next.prev = Arc::downgrade(&node);
			borrowed_node.next = Arc::downgrade(&next);
			if borrowed_next.parent.ptr_eq(&borrowed_node.parent) {
				borrowed_next.prevsib = Arc::downgrade(&node);
			}
		}
		borrowed_after.next = Arc::downgrade(&node);
		borrowed_node.prev = Arc::downgrade(&after);
		borrowed_node.nextsib = borrowed_node.next.clone();
		borrowed_node.prevsib = borrowed_node.prev.clone();
		if !Arc::downgrade(&after).ptr_eq(&borrowed_node.parent) {
			borrowed_after.nextsib = Arc::downgrade(node);
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
		fn parent_prefix(n: &Node, depth: usize, maxdepth: usize) -> String {
			if depth > maxdepth { "".to_string() }
			else {
				match n.parent.upgrade() {
					None => "".to_string(),
					Some(parent) => {
						let ppref = parent_prefix(&parent.lock().expect("Poisoned lock"), depth + 1, maxdepth);
						if parent.lock().expect("Poisoned lock").hide { ppref }
						else if n.last { ppref  + &repeat(" ", COLWIDTH) }
						else { ppref + "│" + &repeat(" ", COLWIDTH - 1) }
					},
				}
			}
		}
		fn cur_prefix(n: &Node, maxdepth: usize) -> String {
			match n.parent.upgrade() {
				None => "".to_string(),
				Some(parent) => {
					let branch = if n.last { "└".to_string() } else { "├".to_string() };
					let ppref = parent_prefix(&parent.lock().expect("Poisoned lock"), 1, maxdepth);
					if parent.lock().expect("Poisoned lock").hide { ppref }
					else { ppref + &branch + &repeat("─", COLWIDTH - 2) + " " }
				}
			}
		}
		match firstline {
			true => cur_prefix(self, maxdepth),
			false => parent_prefix(self, 0, maxdepth),
		}
	}

	pub fn reformat(&mut self, screenwidth: usize) {
		assert!(screenwidth > 0);
		let maxdepth = if self.depth() == 0 { 0 } else { (self.depth() - 1) % ((screenwidth - 1) / COLWIDTH) };
		self.cache.prefix0 = self.prefix(maxdepth, true);
		self.cache.prefix1 = self.prefix(maxdepth, false);
		let contentw = screenwidth - ((maxdepth + 1) * COLWIDTH) % screenwidth;
		self.cache.content = self.value.lock().expect("Poisoned lock").content().format(contentw, super::FG_COLORS.len());
		self.cache.placeholder = self.value.lock().expect("Poisoned lock").placeholder().format(contentw, super::FG_COLORS.len());
		self.cache.search = None;
	}

	fn new(parent: Weak<Mutex<Node<'a>>>, val: Arc<Mutex<Value<'a>>>, width: usize, last: bool, hide: bool) -> Self {
		let mut ret = Node {
			children: vec![],
			parent: parent,
			prev: Weak::new(),
			next: Weak::new(),
			prevsib: Weak::new(),
			nextsib: Weak::new(),
			state: State::Collapsed,
			last: last,
			value: val,
			cache: NodeCache {
				prefix0: "".to_string(),
				prefix1: "".to_string(),
				placeholder: Preformatted::new(0),
				content: Preformatted::new(0),
				search: None,
			},
			hide: hide,
		};
		ret.reformat(width);
		ret
	}

	pub fn new_root(val: Box<dyn BackendValue<'a> + 'a>, width: usize, hide: bool) -> Self {
		Self::new(Weak::new(), Value::new_root(val), width, true, hide)
	}

	fn traverse_unhidden(start: &Arc<Mutex<Node<'a>>>, op: &dyn Fn(&Arc<Mutex<Node<'a>>>) -> Weak<Mutex<Node<'a>>>) -> Weak<Mutex<Node<'a>>> {
		let mut cur = op(&start);
		loop {
			match cur.upgrade() {
				None => return cur,
				Some(node) => {
					if node.lock().expect("Poisoned lock").lines() > 0 { return Arc::downgrade(&node); }
					else { cur = op(&node); }
				},
			}
		}
	}

	pub fn parent(this: &Arc<Mutex<Node<'a>>>) -> Weak<Mutex<Node<'a>>> {
		Self::traverse_unhidden(this, &|n: &Arc<Mutex<Node<'a>>>| n.lock().expect("Poisoned lock").parent.clone())
	}
	
	pub fn next(this: &Arc<Mutex<Node<'a>>>) -> Weak<Mutex<Node<'a>>> {
		Self::traverse_unhidden(this, &|n: &Arc<Mutex<Node<'a>>>| n.lock().expect("Poisoned lock").next.clone())
	}
	
	pub fn prev(this: &Arc<Mutex<Node<'a>>>) -> Weak<Mutex<Node<'a>>> {
		Self::traverse_unhidden(this, &|n: &Arc<Mutex<Node<'a>>>| n.lock().expect("Poisoned lock").prev.clone())
	}
	
	pub fn nextsib(this: &Arc<Mutex<Node<'a>>>) -> Weak<Mutex<Node<'a>>> {
		Self::traverse_unhidden(this, &|n: &Arc<Mutex<Node<'a>>>| n.lock().expect("Poisoned lock").nextsib.clone())
	}

	pub fn prevsib(this: &Arc<Mutex<Node<'a>>>) -> Weak<Mutex<Node<'a>>> {
		Self::traverse_unhidden(this, &|n: &Arc<Mutex<Node<'a>>>| n.lock().expect("Poisoned lock").prevsib.clone())
	}

	pub fn raw_next(&self) -> Weak<Mutex<Node<'a>>> {
		self.next.clone()
	}
	
	pub fn expandable(&self) -> bool {
		self.value.lock().expect("Poisoned lock").expandable()
	}

	fn mark_loading(mut this: &mut Arc<Mutex<Node<'a>>>, width: usize) {
		this.lock().expect("Poisoned lock").children.clear();
		// This is blocked on multi-threading the code, since I want to wait a few milliseconds to
		// see if the children finish loading before taking the time to do a screen redraw to
		// display the loading node.
		/*let val = Value::new_raw(Box::new(StatMsg::new("Loading...".to_string(), 1)), Some(this.lock().expect("Poisoned lock").value.clone()), 0);
		let mut node = Arc::new(Mutex::new(Self::new(Arc::downgrade(this), val, width, true, false)));
		{
			let mut mut_this = this.lock().expect("Poisoned lock");
			mut_this.next = mut_this.nextsib.clone();
			mut_this.children.push(node.clone());
		}
		Self::insert(&mut this, &mut node);*/
		this.lock().expect("Poisoned lock").state = State::Loading;
	}

	fn load_children(this: &mut Arc<Mutex<Node<'a>>>, width: usize) {
		assert!(this.lock().expect("Poisoned lock").state == State::Loading);
		this.lock().expect("Poisoned lock").children.clear();
		let children = Value::children(&this.lock().expect("Poisoned lock").value);
		if children.len() > 0 {
			let lastidx = children.len() - 1;
			for (i, child) in children.into_iter().enumerate() {
				let node = Arc::new(Mutex::new(Self::new(Arc::downgrade(this), child, width, i == lastidx, false)));
				this.lock().expect("Poisoned lock").children.push(node.clone());
			}
		}
	}

	fn finish_loading(this: &mut Arc<Mutex<Node<'a>>>) {
		assert!(this.lock().expect("Poisoned lock").state == State::Loading);
		if let Some(next) = this.lock().expect("Poisoned lock").nextsib.upgrade() {
			next.lock().expect("Poisoned lock").prev = Arc::downgrade(this);
		}
		let nextsib = this.lock().expect("Poisoned lock").nextsib.clone();
		this.lock().expect("Poisoned lock").next = nextsib;
		let mut cur = this.clone();
		let children = this.lock().expect("Poisoned lock").children.iter().cloned().collect::<Vec<Arc<Mutex<Node<'a>>>>>();
		for mut child in children {
			Self::insert(&mut cur, &mut child);
			cur = child.clone();
		}
		this.lock().expect("Poisoned lock").state = State::Expanded;
	}

	pub fn expand(this: &mut Arc<Mutex<Node<'a>>>, width: usize) {
		let (expandable, state) = {
			let locked_this = this.lock().expect("Poisoned lock");
			(locked_this.expandable(), locked_this.state)
		};
		if expandable && state == State::Collapsed {
			Self::mark_loading(this, width);
			Self::load_children(this, width);
			Self::finish_loading(this);
			// The below code should load children in a different thread to avoid blocking the user
			// on slow loads.  Unfortunately, it looks like it's strictly forbidden to send data
			// with non-static lifetimes across threads, and there's no good workaround for this.
			// Hopefully I'll figure it out some day, but until then we're stuck with
			// single-threaded updates.
			/*use std::sync::Condvar;
			use std::thread;
			use std::time::Duration;
			Self::mark_loading(this, width);
			let notify = Arc::new((Mutex::new(0), Condvar::new())); // 0 = still loading, 1 = done loading and caller reloads, 2 = caller exited so thread reloads
			let (thread_this, thread_notify) = (this.clone(), notify.clone());
			thread::spawn(move || {
				let (lock, cond) = &*thread_notify;
				Self::load_children(&mut thread_this, width);
				let mut state = lock.lock().expect("Poisoned lock");
				if *state == 2 {
					Self::finish_loading(&mut thread_this);
					// Callback
				}
				else {
					*state = 1;
					cond.notify_all();
				}
			});
			let (lock, cond) = &*notify;
			let mut state = cond.wait_timeout(lock.lock().expect("Poisoned lock"), Duration::from_millis(1000)).expect("Poisoned lock").0;
			if *state == 1 { Self::finish_loading(this); }
			else { *state = 2 }*/
		}
	}

	pub fn collapse(this: &mut Arc<Mutex<Node>>) {
		let expanded = this.lock().expect("Poisoned lock").state == State::Expanded;
		if expanded {
			this.lock().expect("Poisoned lock").value.lock().expect("Poisoned lock").refresh();
			if let Some(next) = this.lock().expect("Poisoned lock").nextsib.upgrade() {
				next.lock().expect("Poisoned lock").prev = Arc::downgrade(this);
			}
			let mut mut_this = this.lock().expect("Poisoned lock");
			mut_this.next = mut_this.nextsib.clone();
			mut_this.children.clear();
			mut_this.state = State::Collapsed;
		}
	}

	pub fn toggle(this: &mut Arc<Mutex<Node<'a>>>, width: usize) {
		let state = this.lock().expect("Poisoned lock").state;
		match state {
			State::Expanded => Self::collapse(this),
			State::Collapsed => Self::expand(this, width),
			_ => (),
		}
	}

	pub fn recursive_expand(this: &mut Arc<Mutex<Node<'a>>>, width: usize) {
		if this.lock().expect("Poisoned lock").expandable() {
			if this.lock().expect("Poisoned lock").state == State::Collapsed { Self::expand(this, width); }
			let mut children = this.lock().expect("Poisoned lock").children.clone(); // `clone` necessary to prevent a runtime borrow loop
			for child in children.iter_mut() { Self::recursive_expand(child, width); }
		}
	}

	pub fn refresh(this: &mut Arc<Mutex<Node<'a>>>, w: usize) {
		this.lock().expect("Poisoned lock").reformat(w);
		if this.lock().expect("Poisoned lock").state == State::Expanded {
			Self::collapse(this);
			Self::expand(this, w);
		}
	}

	pub fn drawline(&self, palette: &curses::Palette, line: usize, selected: bool) {
		let prefixstr = match line {
			0 => &self.cache.prefix0,
			_ => &self.cache.prefix1,
		};
		let prefix = vec![curses::Output::Fg(1), curses::Output::Str(prefixstr.to_string())];
		let bg = match selected {
			true => 1,
			false => 0,
		};
		let highlight = 2;
		match self.state {
			State::Expanded | State::Loading => self.cache.placeholder.write(line, palette, prefix, bg, highlight, &self.cache.search),
			State::Collapsed => self.cache.content.write(line, palette, prefix, bg, highlight, &self.cache.search),
		}.expect("Failed to write line to terminal");
	}

	pub fn search(&mut self, query: &Option<Regex>) {
		let fmt = match self.state {
			State::Expanded | State::Loading => &self.cache.placeholder,
			State::Collapsed => &self.cache.content,
		};
		if let Some(q) = query {
			if self.cache.search.is_none() || self.cache.search.as_ref().expect("Failed to get content of non-empty option")
				.query().map(|x| x.as_str().to_string()) != Some(q.as_str().to_string()) {
				self.cache.search = Some(fmt.search(q));
			}
		}
		else if self.cache.search.is_some() {
			self.cache.search = None;
		}
	}

	pub fn matches(&self) -> bool {
		match &self.cache.search {
			None => false,
			Some(search) => search.matches(),
		}
	}

	pub fn getsearch(&self) -> &Option<Search> {
		&self.cache.search
	}

	pub fn searchfrom(this: &Arc<Mutex<Node>>, query: &Regex, offset: isize) -> Vec<usize> {
		// If the user provides an enormous offset, that's their problem.  We could choose to first
		// check the number of occurrences and mod by that, but that requires a full document scan,
		// which isn't practical for some backends.
		let value = this.lock().expect("Poisoned lock").value.clone();
		(0..offset.abs()).fold(value, |val, _| {
			Value::searchfrom(&val, query, offset > 0).unwrap_or(val)
		}).lock().expect("Poisoned lock").path()
	}
	
	pub fn is_before(this: &Arc<Mutex<Node>>, n: &Arc<Mutex<Node>>) -> bool {
		let path1 = this.lock().expect("Poisoned lock").value.lock().expect("Poisoned lock").path();
		let path2 = n.lock().expect("Poisoned lock").value.lock().expect("Poisoned lock").path();
		for i in 0..=std::cmp::max(path1.len(), path2.len()) {
			if path2.len() <= i { return false; }
			if path1.len() <= i { return true; }
			if path1[i] > path2[i] { return false; }
			if path1[i] < path2[i] { return true; }
		}
		false
	}

	pub fn is_ancestor_of(this: &Arc<Mutex<Node>>, n: &Arc<Mutex<Node>>) -> bool {
		let path1 = this.lock().expect("Poisoned lock").value.lock().expect("Poisoned lock").path();
		let path2 = n.lock().expect("Poisoned lock").value.lock().expect("Poisoned lock").path();
		if path1.len() >= path2.len() { false }
		else if path2[..path1.len()] != path1[..] { false }
		else { true }
	}

	pub fn invoke(&self) {
		self.value.lock().expect("Poisoned lock").invoke();
	}

	pub fn yank(&self) -> String {
		self.value.lock().expect("Poisoned lock").content().render(interface::Render::Yank, "")
	}
}

impl<'a> std::fmt::Debug for Node<'a> {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		let content = self.value.lock().expect("Poisoned lock").content().render(interface::Render::Debug, " ");
		write!(f, "Node({})", content)
	}
}
