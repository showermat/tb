use std::sync::{Arc, Mutex, Weak};
use super::node::Node;
use std::cmp;

#[derive(Clone)]
pub struct Pos<'a> {
	pub node: Weak<Mutex<Node<'a>>>,
	pub line: usize,
}

impl<'a> Pos<'a> {
	pub fn new(node: Weak<Mutex<Node<'a>>>, line: usize) -> Self {
		Pos { node: node, line: line }
	}

	pub fn nil() -> Self {
		Pos { node: Weak::new(), line: 0 }
	}

	// The following three functions, while more elegantly written recursively, lead to stack overflows in large trees
	pub fn dist_fwd(&self, to: Pos<'a>) -> Option<usize> {
		let mut ret = 0;
		let mut cur = self.clone();
		while !cur.node.ptr_eq(&to.node) {
			match cur.node.upgrade() {
				None => return None,
				Some(n) => {
					ret += n.lock().expect("Poisoned lock").lines() - cur.line;
					cur = Pos::new(n.lock().expect("Poisoned lock").raw_next().clone(), 0);
				},
			}
		}
		if ret + to.line >= cur.line { Some(ret + to.line - cur.line) }
		else { None }
	}

	pub fn fwd(&self, n: usize, safe: bool) -> Self {
		let mut cur = self.clone();
		let mut remain = n;
		loop {
			match cur.node.upgrade() {
				None => return Pos::nil(),
				Some(node) => {
					let curlines = node.lock().expect("Poisoned lock").lines();
					if remain < curlines - cur.line { break; }
					match Node::next(&node).upgrade() {
						None => match safe {
							false => return Pos::nil(),
							true => return Pos::new(cur.node, cmp::max(curlines, 1) - 1),
						},
						Some(realnext) => {
							remain -= curlines - cur.line;
							cur = Pos::new(Arc::downgrade(&realnext), 0);
						}
					}
				}
			}
		}
		Pos::new(cur.node, cur.line + remain)
	}

	pub fn bwd(&self, n: usize, safe: bool) -> Self {
		let mut cur = self.clone();
		let mut remain = n;
		loop {
			match cur.node.upgrade() {
				None => return Pos::nil(),
				Some(node) => {
					if remain <= cur.line { break; }
					match Node::prev(&node).upgrade() {
						None => {
							match safe {
								false => return Pos::nil(),
								true => return Pos::new(cur.node, 0),
							}
						},
						Some(prev) => {
							remain -= cur.line + 1;
							cur = Pos::new(Arc::downgrade(&prev), cmp::max(prev.lock().expect("Poisoned lock").lines(), 1) - 1)
						}
					}
				}
			}
		}
		Pos::new(cur.node, cur.line - remain)
	}

	pub fn seek(&self, n: isize, safe: bool) -> Self {
		if n > 0 { self.fwd(n as usize, safe) }
		else if n < 0 { self.bwd(-n as usize, safe) }
		else { self.clone() }
	}
}
