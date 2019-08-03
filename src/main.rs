#[macro_use]
extern crate error_chain;
extern crate serde_json;
extern crate regex;

mod errors {
	error_chain! { }
}

mod interface;
mod display;
mod keybinder;
mod curses;
mod prompt;
mod backends;
mod format;

use errors::*;
use interface::*;
use std::collections::HashMap;
use std::path::PathBuf;

/*
 * TODO:
 *     Searches with fs backend are very slow and peg a core.  Figure out and eliminate the bottleneck.
 *     Don't assume that every call to backend `children()` returns the same list -- make a derpy test backend that gives random results to test how well this works
 *         Add a child cache to display::Value and use it for all child requests.  Also add a refresh() function and find strategic places to call it from the displaytree.  (Also add keybinding 'r' to explicitly refresh.)
 *     TODOs, FIXMEs, release cleanup
 * Future:
 *     Fun example backends: Reddit, Hacker News https://hacker-news.firebaseio.com/v0/item/18918215.json https://hacker-news.firebaseio.com/v0/topstories.json
 *     Configure: colors, key bindings, tab and indentation sizes, whether to search with regex, mouse scroll multiplier, backend regex
 *     jq integration: https://crates.io/crates/json-query
 * Ideas:
 *     ncurses replacement: https://github.com/TimonPost/crossterm https://github.com/redox-os/termion
 * Bugs:
 *     Serde doesn't give us object elements in document order.  Is there any way to achieve this?
 */

const APPNAME: &str = "tb";

// Borrowed with thanks from clap <https://kbknapp.github.io/clap-rs/clap/macro.crate_version!.html>
macro_rules! crate_version {
	() => {
		format!("{}.{}.{}{}",
			env!("CARGO_PKG_VERSION_MAJOR"),
			env!("CARGO_PKG_VERSION_MINOR"),
			env!("CARGO_PKG_VERSION_PATCH"),
			option_env!("CARGO_PKG_VERSION_PRE").unwrap_or(""))
	}
}

enum BackendSource {
	Builtin,
	File(PathBuf),
}

impl BackendSource {
	pub fn to_string(&self) -> String {
		match self {
			BackendSource::Builtin => "built-in".to_string(),
			BackendSource::File(path) => format!("from file {}", path.as_os_str().to_string_lossy()),
		}
	}
}

type BackendMap = HashMap<String, (Box<Factory>, BackendSource)>;

fn info_exit(backends: BackendMap) {
	let mut sorted = backends.into_iter().collect::<Vec<(String, (Box<Factory>, BackendSource))>>();
	sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).expect("Strings were not partially ordered"));
	let backend_fmt = sorted.into_iter().map(|(_, (v, src))| {
		let info = v.info();
		format!("    {: <12}{} ({})", info.name, info.desc, src.to_string())
	}).collect::<Vec<String>>().join("\n");
	print!(r#"{} {}
Command-line interactive browser for JSON and other tree-structured data
Copyright (GPLv3) 2019 Matthew Schauer <https://github.com/showermat/tb>

Usage: tb help|config|<backend> [backend args...]

Available backends:
{}
"#, APPNAME, crate_version!(), backend_fmt);
	std::process::exit(0);
}

fn run() -> Result<()> {
	let backends: BackendMap = vec![
		backends::json::get_factory(),
		backends::fs::get_factory(),
	].into_iter().map(|x| (x.info().name.to_string(), (x, BackendSource::Builtin))).collect();

	let backend_re = regex::Regex::new("^([a-z]+)b$").chain_err(|| "Invalid regular expression given for backend extraction")?;
	let args_owned = std::env::args().collect::<Vec<String>>();
	let args = args_owned.iter().map(|x| x.as_str()).collect::<Vec<&str>>();
	let (backend, subargs) =
		if args.len() == 0 {
			info_exit(backends);
			unreachable!();
		}
		else {
			let mypath = PathBuf::from(args[0]);
			let callname = mypath.file_name().and_then(|x| x.to_str()).unwrap_or("");
			if callname == APPNAME || !backend_re.is_match(callname) {
				if args.len() == 1 || ["help", "-h", "--help"].contains(&args[1]) {
					info_exit(backends);
					unreachable!();
				}
				else if args[1] == "config" {
					unimplemented!(); // TODO
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

	let factory = &backends.get(&backend).ok_or(format!("Could not find backend \"{}\"", backend))?.0;
	if let Some(treeres) = factory.from(subargs) {
		let tree = treeres?;
		curses::setup();
		let mut dt = display::Tree::new(tree.root(), factory.colors());
		dt.interactive();
		curses::cleanup();
	};
	Ok(())
}

quick_main!(run);
