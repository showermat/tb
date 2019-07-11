#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate clap;
extern crate serde_json;
extern crate regex;

mod errors {
	error_chain! { }
}

mod format;
mod disptree;
mod keybinder;
mod curses;
mod prompt;
mod jsonvalue;
mod fsvalue;

use errors::*;
use disptree::*;
use jsonvalue::*;

/*
 * TODO:
 *     Delay in mouse events
 *     Replace all safe unwraps with expects
 *     TODOs, FIXMEs, and `unwrap()`s
 * Future:
 *     Pluggable backends https://michael-f-bryan.github.io/rust-ffi-guide/dynamic_loading.html https://github.com/Zaerei/rust_plugin_playground
 *         Reddit, Hacker News https://hacker-news.firebaseio.com/v0/item/18918215.json https://hacker-news.firebaseio.com/v0/topstories.json
 *     Customize colors and key bindings
 *     jq integration: https://crates.io/crates/json-query
 * Ideas:
 *     ncurses replacement: https://github.com/TimonPost/crossterm https://github.com/redox-os/termion
 * Bugs:
 *     Serde doesn't give us object elements in document order.  Is there any way to achieve this?
 */

fn run() -> Result<()> {
	let args = clap_app!(jsonb =>
		(version: crate_version!())
		(about: "Command-line interactive JSON browser")
		(@arg file:  index(1) "The file to read")
	).get_matches();

	let json: Box<JsonSource> = match args.value_of("file") {
		Some(fname) => JsonSource::read(std::io::BufReader::new(std::fs::File::open(fname).chain_err(|| "could not open file")?)),
		None => {
			let stdin = std::io::stdin();
			let inlock = stdin.lock();
			JsonSource::read(inlock)
		},
	}.chain_err(|| "failed to load input data")?;
	curses::setup();
	let mut dt = DispTree::new(Box::new(json.root()), json.colors());
	dt.interactive();
	
	/*let fs = fsvalue::FsSource::new(args.value_of("file").unwrap_or("."))?;
	curses::setup();
	let mut dt = DispTree::new(Box::new(fs.root()), fs.colors());
	dt.interactive();*/
	
	curses::cleanup();
	Ok(())
}

quick_main!(run);
