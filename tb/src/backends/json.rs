use ::interface::*;
use ::interface::fmt::*;
use ::serde_json::{from_reader, Value as V};
use anyhow::{Context, Result};

const HI_STR: usize = 0;
const HI_KWD: usize = 1;
const HI_KEY: usize = 2;
const HI_MUT: usize = 3;
const HI_NUM: usize = 4;

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
		super::fmtstr(s, HI_KWD)
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
			V::Number(n) => color(HI_NUM, lit(&n.to_string())),
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

	fn children(&self) -> Vec<Box<dyn Value<'a> + 'a>> {
		match self.value {
			V::Array(items) =>
				items.iter().enumerate().map(|(i, v)| Box::new(JsonValue { key: i.to_string(), value: &v, parent: ParentType::Array }) as Box<dyn Value>).collect(),
			V::Object(items) =>
				items.iter().map(|(k, v)| Box::new(JsonValue { key: k.to_string(), value: &v, parent: ParentType::Object }) as Box<dyn Value>).collect(),
			_ => vec![],
		}
	}
}

pub struct JsonSource {
	json: V,
}

impl JsonSource {
	pub fn read<T: std::io::Read>(input: T) -> Result<Box<dyn Source>> {
		Ok(Box::new(Self { json: from_reader(input).with_context(|| "could not parse input as JSON")? }))
	}
}

impl Source for JsonSource {
	fn root<'a>(&'a self) -> Box<dyn Value<'a> + 'a> {
		Box::new(JsonValue { key: "root".to_string(), value: &self.json, parent: ParentType::Root })
	}

	fn transform(&self, transformation: &str) -> Result<Box<dyn Source>> {
		let result = jq_rs::run(transformation, &self.json.to_string()).map_err(|e| anyhow!("JQ filter failed: {}", e))?;
		Ok(Box::new(Self { json: serde_json::from_str(&result).with_context(|| "JQ returned invalid JSON")? }))
	}
}

pub struct JsonFactory { }

impl Factory for JsonFactory {
	fn info(&self) -> Info {
		Info { name: "j", desc: "Browse JSON documents" }
	}

	fn from<'a>(&self, args: &[&str]) -> Option<Result<Box<dyn Source>>> {
		match args.get(0) {
			Some(&"-h") | Some(&"--help") => {
				print!(r#"jb: Browse JSON documents interactively

Provide the name of the input file to read as the sole command-line argument, or
provide no arguments to read from standard input.

Part of Tree Browser <https://github.com/showermat/tb>
Copyright (GPLv3) 2020 Matthew Schauer
"#);
				None
			},
			Some(fname) => Some(std::fs::File::open(fname).with_context(|| "could not open file").and_then(|file| JsonSource::read(std::io::BufReader::new(file)))),
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
			Color { c8: 6, c256: 204 }, // number
		]
	}
}

pub fn get_factory() -> Box<dyn Factory> {
	Box::new(JsonFactory { })
}
