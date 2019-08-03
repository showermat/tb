use std::rc::Rc;
use std::cell::RefCell;
use ::regex::Regex;

type BackendValue<'a> = Box<::interface::Value<'a> + 'a>;

// TODO In order to avoid producing multiple different instances of a child when calling
// children(), we may want to keep an array of Weaks in the struct and only call children() if the
// element we're looking for is missing.  Not sure whether this is useful at all.
pub struct Value<'a> {
	pub v: BackendValue<'a>,
	pub parent: Option<Rc<RefCell<Value<'a>>>>,
	pub index: usize,
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
		Rc::new(RefCell::new(Value { v: v, parent: None, index: 0 }))
	}

	pub fn children(this: &Ref<'a>) -> Vec<Ref<'a>> {
		if this.borrow().v.expandable() {
			this.borrow().v.children().into_iter().enumerate()
				.map(|(i, child)| Rc::new(RefCell::new(Value { v: child, parent: Some(this.clone()), index: i }))).collect()
		}
		else {
			vec![]
		}
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
			if cur.borrow().v.content().contains(query) {
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
