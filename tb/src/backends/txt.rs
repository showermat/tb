use std::process::{Command, Stdio};
use std::io::{Read, Write};
use std::rc::Rc;
use ::interface::*;
use ::errors::*;

pub struct TxtValue {
	v: String,
}

impl TxtValue {
	fn new(s: String) -> Self {
		Self { v: s }
	}
}

impl<'a> Value<'a> for TxtValue {
	fn content(&self) -> Format { super::fmtstr(&self.v, 0) }

	fn expandable(&self) -> bool { false }

	fn children(&self) -> Vec<Box<dyn Value<'a> + 'a>> { unreachable!(); }
}

pub struct TxtSource {
	buf: Rc<String>,
	sep: String,
}

impl Source for TxtSource {
	fn root<'a>(&'a self) -> Box<dyn Value<'a> + 'a> {
		Box::new(TxtSource { buf: Rc::clone(&self.buf), sep: self.sep.clone() })
	}

	fn transform(&self, transformation: &str) -> errors::Result<Box<dyn Source>> {
		if transformation == "" { Ok(Box::new(TxtSource { buf: self.buf.clone(), sep: self.sep.clone() })) }
		else {
			let mut proc = Command::new("bash").args(vec!["-c", transformation]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().chain_err(|| "Failed to spawn tranform command")?;
			let instream = proc.stdin.as_mut().chain_err(|| "Couldn't get input handle to transform command")?;
			instream.write_all(self.buf.as_bytes()).chain_err(|| "Failed to send input to transform command")?;
			let output = proc.wait_with_output().chain_err(|| "Couldn't get output from transform command")?;
			if !output.status.success() { bail!(String::from_utf8_lossy(&output.stderr).to_string()) }
			Ok(Box::new(TxtSource { buf: Rc::new(String::from_utf8_lossy(&output.stdout).to_string()), sep: self.sep.clone() }))
		}
	}
}

impl<'a> Value<'a> for TxtSource {
	fn content(&self) -> Format { fmt::lit("") }

	fn expandable(&self) -> bool { true }

	fn children(&self) -> Vec<Box<dyn Value<'a> + 'a>> {
		self.buf.split(&self.sep).map(|x| Box::new(TxtValue::new(x.to_string())) as Box<dyn Value<'a> + 'a>).collect()
	}
}

pub struct TxtFactory { }

impl Factory for TxtFactory {
	fn info(&self) -> Info {
		Info { name: "txt", desc: "View and manipulate arbitrarily structured text data" }
	}

	fn from(&self, args: &[&str]) -> Option<Result<Box<dyn Source>>> {
		let mut sep = "\n".to_string();
		let err = match args {
			&[] => None,
			&["-h"] | &["--help"] => {
				print!(r#"
txtb: Browse arbitrarily structured text data.  Provide input on standard input.

Usage: txtb [-s SEP]

Arguments:
-s SEP:  Use SEP as the separator between lines of text
"#);
				Some(None)
			},
			&["-s", s] => {
				sep = s.to_string();
				None
			},
			_ => Some(Some(Err(Error::from("Unrecognized arguments".to_string())))),
		};
		match err {
			Some(e) => e,
			None => {
				let stdin = std::io::stdin();
				let mut inlock = stdin.lock();
				let mut buf = vec![];
				match inlock.read_to_end(&mut buf).chain_err(|| "Couldn't read stdin") {
					Ok(_) => Some(Ok(Box::new(TxtSource { buf: Rc::new(String::from_utf8_lossy(&buf).to_string()), sep: sep }))),
					Err(e) => Some(Err(e)),
				}
			},
		}
	}

	fn colors(&self) -> Vec<Color> {
		vec![
			Color { c8: 4, c256: 244 }, // Control characters
		]
	}

	fn settings(&self) -> Settings {
		Settings {
			hide_root: true,
		}
	}
}

pub fn get_factory() -> Box<dyn Factory> {
	Box::new(TxtFactory { })
}
