use std::collections::HashMap;
use std::path::{PathBuf, Path};
use std::ffi::{OsStr, OsString};
use ::interface::*;
use ::errors::*;

#[derive(PartialEq, Eq, Hash, Clone, Copy)]
pub enum Kind {
	Dir,
	File,
	DirLink,
	FileLink,
	Special,
	Inaccessible,
	Meta,
}

pub struct FsValue {
	name: String,
	path: PathBuf,
	kind: Kind,
}

impl FsValue {
	pub fn colors() -> Vec<(Kind, Color)> { // Is there a way to make a static value so this isn't recomputed every time?
		[
			(Kind::Dir, 27, 4),
			(Kind::File, 231, 7),
			(Kind::DirLink, 51, 6),
			(Kind::FileLink, 51, 6),
			(Kind::Special, 226, 3),
			(Kind::Inaccessible, 196, 1),
			(Kind::Meta, 244, 7),
		].into_iter().map(|(t, c256, c8)| (*t, Color { c8: *c8, c256: *c256 })).collect()
	}

	fn metavalue<'a>(msg: &str) -> Box<Value<'a> + 'a> {
		Box::new(FsValue { name: format!("({})", msg), path: PathBuf::new(), kind: Kind::Meta }) as Box<Value<'a> + 'a>
	}

	fn new(path: &Path) -> Self {
		let name = match path.file_name () {
			Some(name) => name,
			None => OsStr::new("/"), // Because the path is canonical, this should be the only case where `file_name` is `None`
		}.to_os_string().to_string_lossy().to_string();
		let kind = match path.symlink_metadata() {
			Ok(ref m) if m.is_dir() => Kind::Dir,
			Ok(ref m) if m.is_file() => Kind::File,
			Ok(ref m) if m.file_type().is_symlink() => {
				match std::fs::metadata(path) {
					Ok(ref m) if m.is_dir() => Kind::DirLink,
					Ok(_) => Kind::FileLink,
					_ =>  Kind::Inaccessible,
				}
			},
			Ok(_) => Kind::Special,
			Err(_) => Kind::Inaccessible,
		};
		Self { name: name, path: path.to_path_buf(), kind: kind }
	}
}

impl<'a> Value<'a> for FsValue {
	fn content(&self) -> Format {
		let color_ids = Self::colors().iter().enumerate().map(|(i, (t, _))| (t.clone(), i)).collect::<HashMap<Kind, usize>>();
		fmt::color(color_ids[&self.kind], fmt::lit(&self.name))
	}

	fn expandable(&self) -> bool {
		match self.kind {
			Kind::Dir | Kind::DirLink => true,
			_ => false,
		}
	}

	fn children(&self) -> Vec<Box<Value<'a> + 'a>> {
		assert!(self.kind == Kind::Dir || self.kind == Kind::DirLink);
		match std::fs::read_dir(&self.path) {
			Ok(entries) => {
				let mut items = entries.collect::<Vec<std::io::Result<std::fs::DirEntry>>>();
				match items.is_empty() {
					true => vec![Self::metavalue("empty")],
					false => {
						items.sort_by_key(|x| {
							match x {
								Ok(f) => f.file_name(),
								Err(_) => OsString::new(),
							}
						});
						items.into_iter().map(|entry| {
							match entry {
								Ok(f) => Box::new(FsValue::new(&f.path())),
								Err(_) => Self::metavalue("inaccessible"),
							}
						}).collect::<Vec<Box<Value<'a> + 'a>>>()
					}
				}
			},
			Err(_) => vec![Self::metavalue("inaccessible")],
		}
	}

	fn invoke(&self) {
		if self.kind == Kind::File || self.kind == Kind::FileLink {
			let path = self.path.as_os_str().to_os_string();
			std::thread::spawn(move || {
				let _ = std::process::Command::new("xdg-open").arg(path).status();
			});
		}
	}
}

pub struct FsSource {
	root: PathBuf,
}

impl Source for FsSource {
	fn root<'a>(&'a self) -> Box<Value<'a> + 'a> {
		Box::new(FsValue::new(&self.root))
	}
}

pub struct FsFactory { }

impl Factory for FsFactory {
	fn info(&self) -> Info {
		Info { name: "fs", desc: "Browse the file system" }
	}

	fn from(&self, args: &[&str]) -> Option<Result<Box<Source>>> {
		if args.len() == 1 && ["-h", "--help"].contains(&args[0]) {
			println!("fsb: Browse the file system interactively");
			None
		}
		else {
			let root = PathBuf::from(args.get(0).cloned().unwrap_or(".")).canonicalize().chain_err(|| "Couldn't read requested path");
			Some(root.map(|r| Box::new(FsSource { root: r }) as Box<Source>))
		}
	}

	fn colors(&self) -> Vec<Color> {
		FsValue::colors().iter().map(|(_, c)| *c).collect::<Vec<Color>>()
	}
}

pub fn get_factory() -> Box<Factory> {
	Box::new(FsFactory { })
}
