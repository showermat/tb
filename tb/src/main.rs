#[macro_use]
extern crate error_chain;
extern crate serde_json;
extern crate regex;
extern crate libloading;
extern crate itertools;
extern crate clipboard;
extern crate owning_ref;
extern crate tb_interface as interface;
extern crate textproto;

mod display;
mod keybinder;
mod curses;
mod prompt;
mod backends;
mod format;

use interface::*;
use interface::errors::*;
use std::collections::HashMap;
use std::path::PathBuf;
use itertools::Itertools;
use libloading::Library;

const APPNAME: &str = "tb";

macro_rules! crate_version { // Borrowed with thanks from clap <https://kbknapp.github.io/clap-rs/clap/macro.crate_version!.html>
	() => {
		format!("{}.{}.{}{}",
			env!("CARGO_PKG_VERSION_MAJOR"),
			env!("CARGO_PKG_VERSION_MINOR"),
			env!("CARGO_PKG_VERSION_PATCH"),
			option_env!("CARGO_PKG_VERSION_PRE").unwrap_or(""))
	}
}

fn extract_errors<T>(vec: Vec<Result<T>>) -> (Vec<T>, Vec<Error>) {
	let (oks, errs): (Vec<Result<T>>, Vec<Result<T>>) = vec.into_iter().partition(|x| x.is_ok());
	let unwrapped_oks: Vec<T> = oks.into_iter().map(|x| x.expect("Couldn't unwrap ok value")).collect();
	let unwrapped_errs: Vec<Error> = errs.into_iter().map(|x| x.map(|_| ()).expect_err("Couldn't unwrap err value")).collect(); // I'm not sure why `expect_err` requires `T: Debug`, but I don't wan't to require that from all users of this function, so we map the (now non-existent) `Ok` values to nulls.
	(unwrapped_oks, unwrapped_errs)
}

enum BackendSource {
	BuiltIn,
	FromFile(PathBuf),
}

impl BackendSource {
	fn to_string(&self) -> String {
		match self {
			BackendSource::BuiltIn => "built-in".to_string(),
			BackendSource::FromFile(p) => format!("from {}", p.to_string_lossy()),
		}
	}
}

struct Backend {
	source: BackendSource,
	factory: Box<dyn Factory>,
}

impl Backend {
	fn builtin(f: Box<dyn Factory>) -> Self {
		Self { source: BackendSource::BuiltIn, factory: f }
	}
	fn fromfile(path: PathBuf, f: Box<dyn Factory>) -> Self {
		Self { source: BackendSource::FromFile(path), factory: f }
	}
}

fn load_plugins() -> Result<Vec<Result<(PathBuf, Library)>>> {
	let dir = std::env::var("XDG_DATA_HOME").or(std::env::var("HOME").map(|home| home + "/.local/share")).chain_err(|| "Couldn't find XDG data home")? + "/" + APPNAME + "/plugins";
	let entries = std::fs::read_dir(dir.clone()).chain_err(|| format!("{} does not exist", dir))?
		.filter_map(|res| res.ok()).filter(|x| x.path().metadata().map(|y| y.is_file()).unwrap_or(false));
	Ok(entries.map(|entry| libloading::Library::new(&entry.path()).map(|x| (entry.path(), x)).chain_err(|| format!("Failed to open {} as shared library", entry.path().to_string_lossy()))).collect())
}

fn info_exit(backends: HashMap<String, Backend>, errors: Vec<Error>) {
	let backend_fmt = backends.into_iter()
		.sorted_by(|a, b| a.0.partial_cmp(&b.0).expect("Strings are not partially ordered"))
		.map(|(name, backend)| format!("    {: <12}{} ({})", name, backend.factory.info().desc, backend.source.to_string())).join("\n");
	print!(r#"{} {}
Command-line interactive browser for JSON and other tree-structured data
Copyright (GPLv3) 2020 Matthew Schauer <https://github.com/showermat/tb>

Usage: {} help|<backend> [backend args...]

Available backends:
{}
"#, APPNAME, crate_version!(), APPNAME, backend_fmt);
	if errors.len() > 0 {
		println!("\nLoad errors:");
		for err in errors {
			let mut chain = err.iter();
			println!("    {}", chain.next().expect("Error is empty chain").to_string());
			for elem in chain {
				println!("        Caused by: {}", elem);
			}
		}
	}
	std::process::exit(0);
}

fn run() -> Result<()> {
	let builtin_backends = vec![
		backends::json::get_factory(),
		backends::fs::get_factory(),
		backends::txt::get_factory(),
		backends::textproto::get_factory(),
	];
	let (plugins, load_errors) = extract_errors(load_plugins().unwrap_or(vec![])); // Do NOT consume `plugins`!  Use `iter`, not `into_iter`.  Otherwise the symbols extracted from it will end up with dangling pointers and you have fun segfault time.
	let (plugin_backends, factory_errors) = extract_errors(plugins.iter().map(|(path, lib)| unsafe {
		let func: Result<libloading::Symbol<unsafe extern fn() -> Vec<Box<dyn Factory>>>> = lib.get(b"get_factories").chain_err(|| format!("Couldn't load symbol `get_factories` from shared library {}", path.to_string_lossy()));
		func.map(move |f| f().into_iter().map(move |factory| Backend::fromfile(path.clone(), factory)))
	}).collect());
	let backends: HashMap<String, Backend> = itertools::concat(vec![
		builtin_backends.into_iter().map(|x| Backend::builtin(x)).collect::<Vec<Backend>>(),
		plugin_backends.into_iter().flatten().collect()
	]).into_iter().map(|x| (x.factory.info().name.to_string(), x)).collect();
	let errors = itertools::concat(vec![load_errors, factory_errors]);

	let backend_re = regex::Regex::new("^([a-z]+)b$").chain_err(|| "Invalid regular expression given for backend extraction")?;
	let args_owned = std::env::args().collect::<Vec<String>>();
	let args = args_owned.iter().map(|x| x.as_str()).collect::<Vec<&str>>();
	let (backend, subargs) =
		if args.len() == 0 {
			info_exit(backends, errors);
			unreachable!();
		}
		else {
			let mypath = PathBuf::from(args[0]);
			let callname = mypath.file_name().and_then(|x| x.to_str()).unwrap_or("");
			if callname == APPNAME || !backend_re.is_match(callname) {
				if args.len() == 1 || ["help", "-h", "--help"].contains(&args[1]) {
					info_exit(backends, errors);
					unreachable!();
				}
				else {
					(args[1].to_string(), &args[2..])
				}
			}
			else {
				let backend = backend_re.captures(callname).expect("Backend regex does not match argument 0").get(1)
					.ok_or("Backend regex does not capture the backend name")?.as_str().to_string();
				(backend.to_string(), &args[1..])
			}
		};

	let factory = &backends.get(&backend).ok_or(format!("Could not find backend \"{}\"", backend))?.factory;
	if let Some(treeres) = factory.from(subargs) {
		let tree = treeres?;
		curses::setup()?;
		let mut dt = display::Tree::new(tree, factory.colors(), factory.settings())?;
		if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| dt.interactive())) {
			let _ = curses::cleanup();
			std::panic::resume_unwind(e);
		}
		curses::cleanup()?;
	};
	Ok(())
}

quick_main!(run);
