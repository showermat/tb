use std::rc::Weak;
use std::cell::RefCell;
use super::node::Node;

#[derive(Clone)]
pub struct Pos<'a> {
	pub node: Weak<RefCell<Node<'a>>>,
	pub line: usize,
}

impl<'a> Pos<'a> {
	pub fn new(node: Weak<RefCell<Node<'a>>>, line: usize) -> Self {
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
					ret += n.borrow().lines() - cur.line;
					cur = Pos::new(n.borrow().next.clone(), 0);
				},
			}
		}
		Some(ret + to.line - cur.line)
	}

	pub fn fwd(&self, n: usize, safe: bool) -> Self {
		let mut cur = self.clone();
		let mut remain = n;
		loop {
			match cur.node.upgrade() {
				None => return Pos::nil(),
				Some(node) => {
					if remain < node.borrow().lines() - cur.line { break; }
					if node.borrow().next.upgrade().is_none() {
						match safe {
							false => return Pos::nil(),
							true => return Pos::new(cur.node, node.borrow().lines() - 1),
						}
					}
					remain -= node.borrow().lines() - cur.line;
					cur = Pos::new(node.borrow().next.clone(), 0);
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
					match node.borrow().prev.upgrade() {
						None => {
							match safe {
								false => return Pos::nil(),
								true => return Pos::new(cur.node, 0),
							}
						},
						Some(prev) => {
							remain -= cur.line + 1;
							cur = Pos::new(node.borrow().prev.clone(), prev.borrow().lines() - 1)
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
