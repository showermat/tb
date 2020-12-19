use std::sync::{Arc, Mutex};
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
	pub parent: Option<Arc<Mutex<Value<'a>>>>,
	pub index: usize,
	childcache: Option<Vec<Arc<Mutex<Value<'a>>>>>,
}

impl<'a> PartialEq for Value<'a> {
	fn eq(&self, other: &Self) -> bool {
		if self.index == other.index {
			match (self.parent.as_ref(), other.parent.as_ref()) {
				(None, None) => true,
				(Some(a), Some(b)) => Arc::ptr_eq(&a, &b),
				_ => false,
			}
		}
		else { false }
	}
}

impl<'a> Eq for Value<'a> { }

type Ref<'a> = Arc<Mutex<Value<'a>>>;

impl<'a> Value<'a> {
	pub fn new_raw(v: BackendValue<'a>, parent: Option<Arc<Mutex<Value<'a>>>>, index: usize) -> Ref<'a> {
		Arc::new(Mutex::new(Value { v: v, parent: parent, index: index, childcache: None }))
	}

	pub fn new_root(v: BackendValue<'a>) -> Ref<'a> {
		Value::new_raw(v, None, 0)
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
			if this.lock().expect("Poisoned lock").v.expandable() {
				this.lock().expect("Poisoned lock").v.children().into_iter().enumerate()
					.map(|(i, child)| Value::new_raw(child, Some(this.clone()), i)).collect()
			}
			else {
				vec![]
			}
		}
		if this.lock().expect("Poisoned lock").childcache.is_none() {
			let cached = Some(getchildren(this));
			this.lock().expect("Poisoned lock").childcache = cached;
		}
		this.lock().expect("Poisoned lock").childcache.clone().expect("No cached children")
	}

	pub fn refresh(&mut self) {
		self.childcache = None;
	}

	fn root(this: &Ref<'a>) -> Ref<'a> {
		match &this.lock().expect("Poisoned lock").parent {
			None => this.clone(),
			Some(parent) => Self::root(parent),
		}
	}

	fn last(this: &Ref<'a>) -> Ref<'a> {
		Self::children(this).last().map(|child| Self::last(child)).unwrap_or(this.clone())
	}

	fn next(this: &Ref<'a>) -> Option<Ref<'a>> {
		fn nextsib<'a>(me: &Ref<'a>) -> Option<Ref<'a>> {
			let parent = me.lock().expect("Poisoned lock").parent.as_ref().cloned();
			match &parent {
				None => None,
				Some(parent) => {
					let siblings = Value::children(&parent);
					let index = me.lock().expect("Poisoned lock").index;
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
		let parent = this.lock().expect("Poisoned lock").parent.as_ref().cloned();
		match &parent {
			None => None,
			Some(parent) => {
				match this.lock().expect("Poisoned lock").index {
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
			if cur.lock().expect("Poisoned lock").content().contains(query) {
				return Some(cur);
			}
			else if Arc::ptr_eq(&cur, this) {
				return None;
			}
		}
	}

	pub fn path(&self) -> Vec<usize> {
		match &self.parent {
			None => vec![],
			Some(parent) => {
				let mut ret = parent.lock().expect("Poisoned lock").path();
				ret.push(self.index);
				ret
			}
		}
	}
}
