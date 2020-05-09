use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;

/* Things I dislike about Rust:
 * This Keybinder class should not need to be generic.  It should accept closures, and those
 * closures should be able to do whatever they need to do, potentially accessing items in the scope
 * in which they were created.  Unfortunately, the borrow checker can't be sure that this is a safe
 * operation, so I'm forced to make Keybinder generic and have the closures called on keypresses
 * accept an instance of the calling class as an argument.  The workaround works in this case, but
 * it sure ain't pretty.
 */
type Action<T> = Rc<RefCell<Box<dyn FnMut(&mut T, &[i32])>>>;

struct Node<T> {
	children: HashMap<i32, Box<Node<T>>>,
	action: Option<Action<T>>,
}

impl<T> Node<T> {
	pub fn new() -> Self {
		Node { children: HashMap::new(), action: None }
	}
	pub fn assign(&mut self, path: &[i32], action: Action<T>) {
		if path.is_empty() { self.action = Some(action); }
		else { (*self.children.entry(path[0]).or_insert(Box::new(Node::new()))).assign(&path[1..], action); }
	}
	pub fn wait(&mut self, t: &mut T, path: &[i32]) -> Vec<i32> {
		if let Some(ref mut a) = self.action { let x: &mut dyn FnMut(&mut T, &[i32]) = &mut *a.borrow_mut(); x(t, path); }
		if self.children.is_empty() { path.to_vec() }
		else {
			ncurses::timeout(4000);
			let next = ncurses::getch();
			ncurses::timeout(-1);
			if next == ncurses::ERR { path.to_vec() }
			else {
				let mut nextpath = path.to_vec();
				nextpath.push(next);
				match self.children.get_mut(&next) {
					Some(child) => child.wait(t, &nextpath),
					None => nextpath.to_vec(),
				}
			}
		}
	}
}

pub struct Keybinder<T> {
	root: Node<T>,
}

impl<'a, T> Keybinder<T> {
	pub fn new() -> Self {
		Keybinder { root: Node::new() }
	}
	pub fn register(&mut self, paths: &[&[i32]], action: Box<dyn FnMut(&mut T, &[i32])>) {
		let ins = Rc::new(RefCell::new(action));
		for path in paths { self.root.assign(path, ins.clone()); }
	}
	pub fn wait(&mut self, t: &mut T) -> Vec<i32> {
		self.root.wait(t, &[])
	}
}
