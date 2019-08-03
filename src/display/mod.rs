use std::rc::{Rc, Weak};

const COLWIDTH: usize = 4;
const RESERVED_FG_COLORS: usize = 2;

fn weak_ptr_eq<T>(a: &Weak<T>, b: &Weak<T>) -> bool { // Shim for Weak::ptr_eq https://github.com/rust-lang/rust/issues/55981
	match (a.upgrade(), b.upgrade()) {
		(Some(x), Some(y)) => Rc::ptr_eq(&x, &y),
		(None, None) => true,
		_ => false,
	}
}

mod value;
mod node;
mod pos;
mod tree;

pub use self::tree::Tree;
