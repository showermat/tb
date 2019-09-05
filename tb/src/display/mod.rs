use std::rc::{Rc, Weak};
use ::interface::Color;

const COLWIDTH: usize = 4;
const FG_COLORS: [Color; 2] = [
	Color { c8: 7, c256: 7 }, // regular
	Color { c8: 4, c256: 244 }, // muted
];
const BG_COLORS: [Color; 3] = [
	Color { c8: 0, c256: 0 }, // regular
	Color { c8: 7, c256: 237 }, // selected
	Color { c8: 3, c256: 88 }, // highlighted
];

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
