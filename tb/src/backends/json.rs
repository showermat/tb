use ::interface::*;
use ::interface::fmt::*;
use ::serde_json::{from_reader, Value as V};
use ::errors::*;

const HI_STR: usize = 0;
const HI_KWD: usize = 1;
const HI_KEY: usize = 2;
const HI_MUT: usize = 3;

#[derive(Clone, Copy, Debug)]
enum ParentType {
	Root,
	Object,
	Array,
}

#[derive(Debug)]
pub struct JsonValue<'a> {
	key: String,
	value: &'a V,
	parent: ParentType,
}

impl<'a> JsonValue<'a> {
	fn fmtstr(s: &str) -> Format {
		let mut parts = vec![];
		let mut cur = "".to_string();
		for c in s.chars() {
			match c as i32 {
				0..=8 | 11..=31 | 127 => {
					let ctrlchar = (((c as i32 + 64) % 128) as u8 as char).to_string();
					parts.extend(vec![lit(&cur), nosearch(nobreak(color(HI_KWD, lit(&("^".to_string() + &ctrlchar)))))]);
					cur = "".to_string();
				},
				_ => cur.push(c),
			};
		}
		if cur.len() > 0 { parts.push(lit(&cur)); }
		cat(parts)
	}

	fn fmtkey(&self) -> Format {
		match self.parent {
			ParentType::Root => nosearch(color(HI_MUT, lit("root"))),
			ParentType::Object => noyank(color(HI_KEY, Self::fmtstr(&self.key))),
			ParentType::Array => hide(color(HI_MUT, Self::fmtstr(&self.key))),
		}
	}

	fn fmtval(&self) -> Format {
		match self.value {
			V::String(s) => color(HI_STR, Self::fmtstr(s)),
			V::Number(n) => color(HI_KWD, lit(&n.to_string())),
			V::Bool(b) => color(HI_KWD, lit(if *b { "true" } else { "false" })),
			V::Object(items) => nosearch(color(HI_KWD, lit(if items.is_empty() { "{ }" } else { "{...}" }))),
			V::Array(items) => nosearch(color(HI_KWD, lit(if items.is_empty() { "[ ]" } else { "[...]" }))),
			V::Null => color(HI_KWD, lit("null")),
		}
	}
}

impl<'a> Value<'a> for JsonValue<'a> {
	fn placeholder(&self) -> Format {
		self.fmtkey()
	}

	fn content(&self) -> Format {
		match self.parent {
			ParentType::Root => self.fmtval(),
			_ => cat(vec![self.fmtkey(), hide(color(HI_MUT, lit(": "))), self.fmtval()]),
		}
	}

	fn expandable(&self) -> bool {
		match *self.value {
			V::Array(_) | V::Object(_) => true,
			_ => false,
		}
	}

	fn children(&self) -> Vec<Box<Value<'a> + 'a>> {
		match self.value {
			V::Array(items) =>
				items.iter().enumerate().map(|(i, v)| Box::new(JsonValue { key: i.to_string(), value: &v, parent: ParentType::Array }) as Box<Value>).collect(),
			V::Object(items) =>
				items.iter().map(|(k, v)| Box::new(JsonValue { key: k.to_string(), value: &v, parent: ParentType::Object }) as Box<Value>).collect(),
			_ => vec![],
		}
	}
}

pub struct JsonSource {
	json: V,
}

impl JsonSource {
	pub fn read<T: std::io::Read>(input: T) -> Result<Box<Source>> {
		Ok(Box::new(Self { json: from_reader(input).chain_err(|| "could not parse input as JSON")? }))
	}
}

impl Source for JsonSource {
	fn root<'a>(&'a self) -> Box<Value<'a> + 'a> {
		Box::new(JsonValue { key: "root".to_string(), value: &self.json, parent: ParentType::Root })
	}
}

pub struct JsonFactory { }

impl Factory for JsonFactory {
	fn info(&self) -> Info {
		Info { name: "j", desc: "Browse JSON documents" }
	}

	fn from<'a>(&self, args: &[&str]) -> Option<Result<Box<Source>>> {
		match args.get(0).cloned() { // TODO Why is the `cloned` here necessary?
			Some("-h") | Some("--help") => {
				print!(r#"jb: Browse JSON documents interactively

Provide the name of the input file to read as the sole command-line argument, or
provide no arguments to read from standard input.

Part of Tree Browser <https://github.com/showermat/tb>
Copyright (GPLv3) 2019 Matthew Schauer
"#);
				None
			},
			Some(fname) => Some(std::fs::File::open(fname).chain_err(|| "could not open file").and_then(|file| JsonSource::read(std::io::BufReader::new(file)))),
			None => {
				let stdin = std::io::stdin();
				let inlock = stdin.lock();
				Some(JsonSource::read(inlock))
			},
		}
	}

	fn colors(&self) -> Vec<Color> {
		vec![
			Color { c8: 2, c256: 77 }, // string
			Color { c8: 1, c256: 214 }, // keyword
			Color { c8: 5, c256: 177 }, // key
			Color { c8: 4, c256: 244 }, // muted
		]
	}
}

pub fn get_factory() -> Box<Factory> {
	Box::new(JsonFactory { })
}