use ::format::FmtCmd;
use ::curses;
use ::errors::*;

pub trait Value<'a> {
	fn placeholder(&self) -> FmtCmd;
	fn content(&self) -> FmtCmd;
	fn expandable(&self) -> bool;
	fn children(&self) -> Vec<Box<Value<'a> + 'a>>;
	fn invoke(&self);
}

/*impl<'a> Value<'a> {
	fn get_children(&self, fwd: bool) -> impl Iterator<Item=(usize, Box<Value<'a> + 'a>)> {
		let mut c =
			if self.expandable() { self.children() }
			else { vec![] };
		if !fwd { c.reverse(); }
		let n = c.len();
		let mut children = c.iter().enumerate();
		iter::from_fn(move || {
			children.next().map(|(i, child)| {
				let idx = if fwd { i } else { n - i - 1 };
				(idx, *child)
			})
		})
	}
	fn dfs_fwd(root: Box<Value<'a> + 'a>, query: &str, start: &[usize]) -> impl Iterator<Item=Vec<usize>> {
		let mut stack: Vec<(Vec<usize>, Box<Value<'a> + 'a>)> = vec![];
		let mut cur: Box<Value<'a> + 'a> = root;
		stack.push((vec![], root));
		let startfull = start.to_vec();
		startfull.push(0); // FIXME -1 in Nim...does it matter?
		for (len, elem) in startfull.iter().enumerate() {
			let children: Box<Iterator<Item=(usize, Box<Value<'a> + 'a>)> + 'a> = Box::new(cur.get_children(false));
			for (idx, child) in cur.get_children(false) {
				if idx == *elem {
					cur = child;
					break;
				}
				let mut path = start[0..len].to_vec();
				path.push(idx);
				stack.push((path, child));
			}
		}
		iter::from_fn(move || {
			if let Some((path, node)) = stack.pop() {
				// ...
				for (i, child) in node.get_children(false) {
					let mut newpath = path;
					newpath.push(i);
					stack.push((newpath, child));
				}
				None
			}
			else { None }
		})
	}
}*/

pub struct Info {
	pub name: &'static str,
	pub desc: &'static str,
}

pub trait Factory {
	fn info(&self) -> Info;
	fn from(&self, &[&str]) -> Option<Result<Box<Source>>>;
	fn colors(&self) -> Vec<curses::Color>;
}

pub trait Source {
	fn root<'a>(&'a self) -> Box<Value<'a> + 'a>;
}
