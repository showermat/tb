use ::tb_interface::*;
use ::errors::Result;
use ::rand::Rng;

pub struct Rand {
	depth: usize,
	value: String,
}

impl<'a> Value<'a> for Rand {
	fn content(&self) -> Format {
		fmt::lit(&self.value)
	}

	fn expandable(&self) -> bool {
		true
	}

	fn children(&self) -> Vec<Box<Value<'a> + 'a>> {
		fn exprand(scale: f32) -> u16 {
			let raw: f32 = rand::thread_rng().sample(::rand_distr::Exp::new(0.5).unwrap());
			(raw * scale) as u16
		}
		fn randstr() -> String {
			let len = exprand(10.0) as usize + 1;
			std::iter::repeat(()).map(|_| rand::thread_rng().sample(rand::distributions::Alphanumeric)).take(len).collect()
		}
		let nchild = match self.depth {
			0 => 5,
			depth => exprand(5.0 / (depth + 1) as f32),
		};
		(0..nchild).map(|_| Box::new(Rand { depth: self.depth + 1, value: randstr() }) as Box<Value<'a> + 'a>).collect()
	}
}

pub struct RandSource { }

impl Source for RandSource {
	fn root<'a>(&'a self) -> Box<Value<'a> + 'a> {
		Box::new(Rand { depth: 0, value: "root".to_string() })
	}
}

pub struct RandFactory { }

impl Factory for RandFactory {
	fn info(&self) -> Info {
		Info { name: "rand", desc: "Create a tree of random and ever-changing nonsense" }
	}

	fn from(&self, _args: &[&str]) -> Option<Result<Box<Source>>> {
		Some(Ok(Box::new(RandSource { })))
	}

	fn colors(&self) -> Vec<Color> {
		vec![]
	}
}
