use std::rc::Rc;
use std::cell::RefCell;
use ::regex::Regex;
use ::interface::Format;
use ::format::FmtCmd;

type BackendValue<'a> = Box<dyn (::interface::Value<'a>) + 'a>;

// FIXME! Both tb and its plugins need to be able to access the FmtCmd type.  However, I don't want
// to include all of the formatting code in FmtCmd's impl, especially since that will require
// drawing in the regex crate, a bunch of curses commands, and a ton of other junk that isn't
// necessary to instantiate FmtCmds, which is all that's necessary from the plugin's side.  I'd
// rather not pull the majority of my display code into the interface library, because I imagine it
// will inflate the sizes of plugins quite a bit.  Until I figure out a more elegant solution, I'm
// providing a dummy struct with the same interface in the interface library, and doing a deep-copy
// conversion to the full-featured struct here.  Ick.
fn fmtcmd_from_format(fmt: Format) -> FmtCmd {
	match fmt {
		Format::Literal(s) => FmtCmd::Literal(s),
		Format::Container(v) => FmtCmd::Container(v.into_iter().map(|x| fmtcmd_from_format(x)).collect()),
		Format::Color(c, v) => FmtCmd::Color(c, Box::new(fmtcmd_from_format(*v))),
		Format::RawColor(c, v) => FmtCmd::RawColor(c, Box::new(fmtcmd_from_format(*v))),
		Format::NoBreak(v) => FmtCmd::NoBreak(Box::new(fmtcmd_from_format(*v))),
		Format::Exclude(r, v) => FmtCmd::Exclude(r, Box::new(fmtcmd_from_format(*v))),
	}
}

pub struct Value<'a> {
	v: BackendValue<'a>,
	pub parent: Option<Rc<RefCell<Value<'a>>>>,
	pub index: usize,
	childcache: Option<Vec<Rc<RefCell<Value<'a>>>>>,
}

impl<'a> PartialEq for Value<'a> {
	fn eq(&self, other: &Self) -> bool {
		self.index == other.index && self.parent == other.parent
	}
}

impl<'a> Eq for Value<'a> { }

type Ref<'a> = Rc<RefCell<Value<'a>>>;

impl<'a> Value<'a> {
	pub fn new_root(v: BackendValue<'a>) -> Ref<'a> {
		Rc::new(RefCell::new(Value { v: v, parent: None, index: 0, childcache: None }))
	}

	pub fn placeholder(&self) -> FmtCmd {
		fmtcmd_from_format(self.v.placeholder())
	}

	pub fn content(&self) -> FmtCmd {
		fmtcmd_from_format(self.v.content())
	}

	pub fn expandable(&self) -> bool {
		self.v.expandable()
	}

	pub fn invoke(&self) {
		self.v.invoke()
	}

	pub fn children(this: &Ref<'a>) -> Vec<Ref<'a>> {
		fn getchildren<'a>(this: &Ref<'a>) -> Vec<Ref<'a>> {
			if this.borrow().v.expandable() {
				this.borrow().v.children().into_iter().enumerate()
					.map(|(i, child)| Rc::new(RefCell::new(Value { v: child, parent: Some(this.clone()), index: i, childcache: None }))).collect()
			}
			else {
				vec![]
			}
		}
		if this.borrow().childcache.is_none() {
			let cached = Some(getchildren(this));
			this.borrow_mut().childcache = cached;
		}
		this.borrow().childcache.clone().expect("No cached children")
	}

	pub fn refresh(&mut self) {
		self.childcache = None;
	}

	fn root(this: &Ref<'a>) -> Ref<'a> {
		match &this.borrow().parent {
			None => this.clone(),
			Some(parent) => Self::root(parent),
		}
	}

	fn last(this: &Ref<'a>) -> Ref<'a> {
		Self::children(this).last().map(|child| Self::last(child)).unwrap_or(this.clone())
	}

	fn next(this: &Ref<'a>) -> Option<Ref<'a>> {
		fn nextsib<'a>(me: &Ref<'a>) -> Option<Ref<'a>> {
			match &me.borrow().parent {
				None => None,
				Some(parent) => {
					let siblings = Value::children(&parent);
					let index = me.borrow().index;
					if index < siblings.len() - 1 {
						Some(siblings[index + 1].clone())
					}
					else {
						nextsib(&parent)
					}
				}
			}
		}
		let children = Self::children(this);
		match children.len() {
			0 => nextsib(this),
			_ => Some(children[0].clone()),
		}
	}

	fn prev(this: &Ref<'a>) -> Option<Ref<'a>> {
		match &this.borrow().parent {
			None => None,
			Some(parent) => {
				match this.borrow().index {
					0 => Some(parent.clone()),
					index => Some(Self::last(&Self::children(&parent)[index - 1])),
				}
			}
		}
	}

	// Yet again, I don't trust the recursive solution of this not to overflow.
	pub fn searchfrom(this: &Ref<'a>, query: &Regex, forward: bool) -> Option<Ref<'a>> {
		let mut cur = this.clone();
		loop {
			let next = if forward { Self::next(&cur) } else { Self::prev(&cur) };
			cur = match next {
				Some(n) => n,
				None => match forward {
					true => Self::root(this),
					false => Self::last(&Self::root(this)),
				},
			};
			if cur.borrow().content().contains(query) {
				return Some(cur);
			}
			else if cur == *this {
				return None;
			}
		}
	}

	pub fn path(&self) -> Vec<usize> {
		match &self.parent {
			None => vec![],
			Some(parent) => {
				let mut ret = parent.borrow().path();
				ret.push(self.index);
				ret
			}
		}
	}
}
