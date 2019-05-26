import os
import streams
import jsonvalue
import displaylist

# TODO
# Multiple backends using concepts or manual interfaces
#     https://nim-lang.org/docs/manual.html#generics-concepts
#     https://openmymind.net/Interfaces-In-Nim/
# Interactive filtering with libjq: https://github.com/stedolan/jq/wiki/C-API:-jq-program-invocation

proc main(args: seq[TaintedString]) =
    let tty = open("/dev/tty", fmReadWrite, 0)
    defer: tty.close()
    case args.len
    of 0:
        newDisplayList(newJsonRoot(newFileStream(stdin)), tty).interactive()
    of 1:
        var f = newFileStream(args[0])
        if f == nil: raise newException(Exception, "Couldn't open file")
        defer: f.close
        newDisplayList(newJsonRoot(f), tty).interactive()
    else: raise newException(Exception, "Too many arguments")

try: main(commandLineParams())
except:
    echo("Error: " & getCurrentExceptionMsg())
    when not defined(release): echo(getCurrentException().getStackTrace())
