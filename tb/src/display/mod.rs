use ::interface::Color;

const COLWIDTH: usize = 4;
const FG_COLORS: [Color; 3] = [
	Color { c8: 7, c256: 7 }, // regular
	Color { c8: 4, c256: 244 }, // muted
	Color { c8: 1, c256: 196 }, // error
];
const BG_COLORS: [Color; 3] = [
	Color { c8: 0, c256: 0 }, // regular
	Color { c8: 7, c256: 237 }, // selected
	Color { c8: 3, c256: 88 }, // highlighted
];

mod value;
mod node;
mod pos;
mod tree;
mod statmsg;

pub use self::tree::Tree;
