use std::rc::{Rc, Weak};
use std::cell::RefCell;
use ::regex::Regex;
use ::format::{Preformatted, Search};
use ::curses;
use super::value::Value;
use ::interface::Value as BackendValue;
use super::COLWIDTH;

struct NodeCache {
	prefix0: String,
	prefix1: String,
	placeholder: Preformatted,
	content: Preformatted,
	search: Option<Search>,
}

pub struct Node<'a> {
	pub children: Vec<Rc<RefCell<Node<'a>>>>,
	pub parent: Weak<RefCell<Node<'a>>>,
	pub prev: Weak<RefCell<Node<'a>>>,
	pub next: Weak<RefCell<Node<'a>>>,
	pub prevsib: Weak<RefCell<Node<'a>>>,
	pub nextsib: Weak<RefCell<Node<'a>>>,
	pub expanded: bool,
	last: bool,
	value: Rc<RefCell<Value<'a>>>,
	cache: NodeCache,
}

impl<'a> Node<'a> {
	pub fn depth(&self) -> usize {
		match self.parent.upgrade() {
			None => 0,
			Some(p) => p.borrow().depth() + 1,
		}
	}

	pub fn lines(&self) -> usize {
		match self.expanded {
			true => self.cache.placeholder.len(),
			false => self.cache.content.len(),
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
	fn insert(after: &mut Rc<RefCell<Node<'a>>>, node: &mut Rc<RefCell<Node<'a>>>) {
		let mut borrowed_node = node.borrow_mut();
		let mut borrowed_after = after.borrow_mut();
		if let Some(next) = borrowed_after.next.upgrade() {
			let mut borrowed_next = next.borrow_mut();
			borrowed_next.prev = Rc::downgrade(&node);
			borrowed_node.next = Rc::downgrade(&next);
			if borrowed_next.parent.ptr_eq(&borrowed_node.parent) {
				borrowed_next.prevsib = Rc::downgrade(&node);
			}
		}
		borrowed_after.next = Rc::downgrade(&node);
		borrowed_node.prev = Rc::downgrade(&after);
		borrowed_node.nextsib = borrowed_node.next.clone();
		borrowed_node.prevsib = borrowed_node.prev.clone();
		if !Rc::downgrade(&after).ptr_eq(&borrowed_node.parent) {
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
		fn parent_prefix(n: &Node, depth: usize, maxdepth: usize) -> String {
			if n.parent.upgrade().is_none() || depth > maxdepth { "".to_string() }
			else {
				let parent = n.parent.upgrade().expect("Couldn't get parent node in prefix");
				if n.last { parent_prefix(&parent.borrow(), depth + 1, maxdepth) + &repeat(" ", COLWIDTH) }
				else { parent_prefix(&parent.borrow(), depth + 1, maxdepth) + "│" + &repeat(" ", COLWIDTH - 1) }
			}
		}
		fn cur_prefix(n: &Node, maxdepth: usize) -> String {
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

	pub fn reformat(&mut self, screenwidth: usize) {
		assert!(screenwidth > 0);
		let maxdepth = if self.depth() == 0 { 0 } else { (self.depth() - 1) % ((screenwidth - 1) / COLWIDTH) };
		self.cache.prefix0 = self.prefix(maxdepth, true);
		self.cache.prefix1 = self.prefix(maxdepth, false);
		let contentw = screenwidth - ((maxdepth + 1) * COLWIDTH) % screenwidth;
		self.cache.content = self.value.borrow().content().format(contentw, super::FG_COLORS.len());
		self.cache.placeholder = self.value.borrow().placeholder().format(contentw, super::FG_COLORS.len());
	}

	fn new(parent: Weak<RefCell<Node<'a>>>, val: Rc<RefCell<Value<'a>>>, width: usize, last: bool) -> Self {
		let mut ret = Node {
			children: vec![],
			parent: parent,
			prev: Weak::new(),
			next: Weak::new(),
			prevsib: Weak::new(),
			nextsib: Weak::new(),
			expanded: false,
			last: last,
			value: val,
			cache: NodeCache {
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

	pub fn new_root(val: Box<dyn BackendValue<'a> + 'a>, width: usize) -> Self {
		Self::new(Weak::new(), Value::new_root(val), width, true)
	}

	pub fn expandable(&self) -> bool {
		self.value.borrow().expandable()
	}

	pub fn expand(this: &mut Rc<RefCell<Node<'a>>>, width: usize) {
		if this.borrow().expandable() && !this.borrow().expanded {
			if Value::children(&this.borrow().value).len() > 0 {
				let mut cur = this.clone();
				let lastidx = Value::children(&this.borrow().value).len() - 1;
				let children = Value::children(&this.borrow().value);
				for (i, child) in children.into_iter().enumerate() {
					let mut node = Rc::new(RefCell::new(Self::new(Rc::downgrade(this), child, width, i == lastidx)));
					this.borrow_mut().children.push(node.clone());
					Self::insert(&mut cur, &mut node);
					cur = node;
				}
			}
			this.borrow_mut().expanded = true;
		}
	}

	pub fn collapse(this: &mut Rc<RefCell<Node>>) {
		let expanded = this.borrow().expanded;
		if expanded {
			this.borrow().value.borrow_mut().refresh();
			/*if let Some(next) = this.borrow().next.upgrade() {
				next.borrow_mut().prev = Rc::downgrade(this);
			}*/
			if let Some(next) = this.borrow().nextsib.upgrade() {
				next.borrow_mut().prev = Rc::downgrade(this);
			}
			let mut mut_this = this.borrow_mut();
			mut_this.next = mut_this.nextsib.clone();
			mut_this.children = vec![];
			mut_this.expanded = false;
		}
	}

	pub fn toggle(this: &mut Rc<RefCell<Node<'a>>>, width: usize) {
		let expanded = this.borrow().expanded;
		match expanded {
			true => Self::collapse(this),
			false => Self::expand(this, width),
		}
	}

	pub fn recursive_expand(this: &mut Rc<RefCell<Node<'a>>>, width: usize) {
		if this.borrow().expandable() {
			if !this.borrow().expanded { Self::expand(this, width); }
			let mut children = this.borrow_mut().children.clone(); // `clone` necessary to prevent a runtime borrow loop
			for child in children.iter_mut() { Self::recursive_expand(child, width); }
		}
	}

	pub fn refresh(this: &mut Rc<RefCell<Node<'a>>>, w: usize) {
		this.borrow_mut().reformat(w);
		if this.borrow().expanded {
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
		match self.expanded {
			true => self.cache.placeholder.write(line, palette, prefix, bg, highlight, &self.cache.search),
			false => self.cache.content.write(line, palette, prefix, bg, highlight, &self.cache.search),
		}.expect("Failed to write line to terminal");
	}

	pub fn search(&mut self, query: &Option<Regex>) {
		let fmt = match self.expanded {
			true => &self.cache.placeholder,
			false => &self.cache.content,
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

	pub fn getsearch(&self) -> &Option<Search> {
		&self.cache.search
	}

	pub fn searchfrom(&self, query: &Regex, offset: isize) -> Vec<usize> {
		// If the user provides an enormous offset, that's their problem.  We could choose to first
		// check the number of occurrences and mod by that, but that requires a full document scan,
		// which isn't practical for some backends.
		(0..offset.abs()).fold(self.value.clone(), |val, _| {
			Value::searchfrom(&val, query, offset > 0).unwrap_or(val)
		}).borrow().path()
	}
	
	pub fn is_before(&self, n: Rc<RefCell<Node>>) -> bool {
		let (path1, path2) = (self.value.borrow().path(), n.borrow().value.borrow().path());
		for i in 0..=std::cmp::max(path1.len(), path2.len()) {
			if path2.len() <= i { return false; }
			if path1.len() <= i { return true; }
			if path1[i] > path2[i] { return false; }
			if path1[i] < path2[i] { return true; }
		}
		false
	}

	pub fn invoke(&self) {
		self.value.borrow().invoke();
	}

	pub fn yank(&self) -> String {
		self.value.borrow().content().render(interface::Render::Yank, "")
	}
}

impl<'a> std::fmt::Debug for Node<'a> {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		let content = self.value.borrow().content().render(interface::Render::Debug, " ");
		write!(f, "Node({})", content)
	}
}
