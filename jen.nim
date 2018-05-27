import os
import streams
import json
import tables
import displaylist
import util

# TODO
# Search
# Switch to ncurses to allow feature detection
# Use macros to avoid code duplication in char trie
# Strings too large to fit on one screen -- we need to not anchor only to the top or bottom of the node
# Control chars in keys and very long keys
# Editing -- start with 'e' to open in external editor
# Mouse?  Click to select, double-click to expand, scroll

proc main(args: seq[TaintedString]) =
    try:
        let tty = open("/dev/tty", fmReadWrite, 0)
        defer: tty.close()
        case args.len
        of 0:
            var f = stdin.newFileStream.parseJson
            newDisplayList(f, tty).interactive()
        of 1:
            var f = newFileStream(args[0])
            if f == nil: raise newException(Exception, "Couldn't open file")
            defer: f.close
            newDisplayList(f.parseJson, tty).interactive()
        else: raise newException(Exception, "Too many arguments")
    except JsonParsingError:
        echo("Failed parsing JSON: " & getCurrentException().msg)
        quit(1)
    except:
        echo(getCurrentException().msg)
        quit(2)

main(commandLineParams())
