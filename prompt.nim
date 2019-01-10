import util
import unicode
import strutils

# TODO
# Handle when input (or prompt) is longer than width

proc ignore(x: string) = discard

proc graphwidth(buf: seq[Rune]): int = # TODO Allow this to accept bounds so that we don't have to pre-slice the string we pass in
    var ret = 0
    for rune in buf: ret += max(rune.wcwidth, 0)
    return ret

proc prompt*(tty: File, location: (int, int), width: int, prompt: string = "", init: string = "", history: seq[string] = @[], callback: proc(x: string) = ignore): string =
    proc goto(y: int, x: int): string = "\e[" & $y & ";" & $x & "H"
    proc goto(loc: (int, int)): string = goto(loc[0], loc[1])
    let promptwidth = prompt.toRunes.graphwidth
    var buf: seq[Rune] = init.toRunes
    var pos = buf.len
    proc move(offset: int) =
        if offset == 0: return
        let newpos = max(min(pos + offset, buf.len), 0)
        let delta = newpos - pos
        if delta > 0: tty.write("\e[" & $buf[pos..<pos + delta].graphwidth & "C")
        elif delta < 0: tty.write("\e[" & $buf[pos + delta..<pos].graphwidth & "D")
        pos = newpos
    proc doCallback() =
        #echo($buf & "|")
        callback($buf)
        tty.write(goto(location[0], location[1] + promptwidth + buf[0..<pos].graphwidth))
    tty.write(goto(location) & prompt & " \e[" & $(width - promptwidth - 1) & "b" & goto(location[0], location[1] + promptwidth) & "\e[?25h\e[?1000l")
    tty.write($buf)
    defer: tty.write("\e[?25l\e[?1000h")
    doCallback()
    while true:
        let key = tty.readchar()
        case key:
        of '\x0d': return $buf # Enter
        of '\x7f': # Backspace
            if pos <= 0: continue
            let rmwidth = buf[pos - 1].wcwidth
            buf.delete(pos - 1)
            tty.write("\e[" & $rmwidth & "D" & $buf[pos - 1..^1] & " ".repeat(rmwidth) & "\e[" & $rmwidth & "D")
            pos -= 1
            doCallback()
        of '\x01': move(-pos) # ^A
        of '\x05': move(buf.len - pos) # ^E
        of '\x1b': # Escape
            let (cmd, args) = ctlseq(tty)
            case cmd
            of '\0': return nil
            of 'C': move(1) # Right
            of 'D': move(-1) # Left
            of '~':
                if args == @[3]: # Delete
                    if pos >= buf.len: continue
                    let rmwidth = buf[pos].wcwidth
                    buf.delete(pos)
                    tty.write($buf[pos..^1] & " ".repeat(rmwidth))
                    doCallback()
            else: discard
        else:
            var input = $key
            if (key.uint8 and 0x80) > 0.uint: # UTF-8 input
                var charlen = 0
                if (key.uint8 and 0xf8) == 0xf0: charlen = 3
                elif (key.uint8 and 0xf0) == 0xe0: charlen = 2
                elif (key.uint8 and 0xe0) == 0xc0: charlen = 1
                for i in 1..charlen: input &= tty.readchar()
            assert input.runeAt(0).toUTF8.len == input.len
            buf.insert(input.runeAt(0), pos)
            tty.write(buf[pos..^1])
            pos += 1
            doCallback()
