import util
import unicode
import strutils

let fgMuted = newColorSpec(244, 4, false)

proc ignore(x: string) = discard

proc isctrl(c: Rune): bool =
    return c.int32 < 32 or c.int32 == 127

proc graphwidth(c: Rune): int =
    if isctrl(c): return 2
    return max(c.wcwidth, 0)

proc graphwidth(buf: seq[Rune]): int =
    var ret = 0
    for c in buf: ret += c.graphwidth
    return ret

proc printchar(c: Rune): string =
    if isctrl(c):
        if c.int32 == 127: return colorPrint(fgMuted, "^?")
        return colorPrint(fgMuted, "^" & (c.int32 + 64).chr)
    return $c

proc prompt*(tty: File, location: (int, int), width: int, prompt: string = "", init: string = "", history: seq[string] = @[], callback: proc(x: string) = ignore): string =
    let promptw = prompt.toRunes.graphwidth
    let effw = width - promptw # Width of buffer view
    assert effw > 0 # TODO Handle this more gracefully
    proc goto(xoff: int): string = "\e[" & $location[0] & ";" & $(location[1] + promptw + xoff) & "H"
    proc repeat(c: char, n: int): string =
        if n == 0: ""
        elif n == 1: $c
        else: $c & "\e[" & $(n - 1) & "b"
    var histedit = history & init
    var histidx = histedit.len - 1
    var buf: seq[Rune]
    var pos: int # Cursor position in buffer
    var offset: int = 0 # First visible character
    var dispw: int = 0 # Graphical width of displayed portion of buffer
    var dispn: int = 0 # Number of characters displayed
    proc doCallback() =
        callback($buf)
        tty.write(goto(buf[offset..<pos].graphwidth))
    proc drawFrom(idx: int) =
        let start = max(idx, offset)
        var ret = ""
        #assert start < buf.len
        var w = buf[offset..start - 1].graphwidth
        for c in buf[start..^1]:
            let curw = c.graphwidth
            if w + curw > effw: break
            w += curw
            ret &= printchar(c)
        tty.write(ret & repeat(' ', effw - w)) # TODO Maybe optimize with rmwidth?
    proc move(by: int) =
        let ndelta = min(max(pos + by, 0), buf.len) - pos
        let delta = ndelta.abs
        if ndelta > 0:
            if dispn < buf.len and pos + delta >= offset + dispn: # We're going off the right end
                let dispend = if pos + delta < buf.len: buf[pos + delta].graphwidth - 1 else: 0
                dispw = dispend
                dispn = 0
                for i in countdown(pos + delta - 1, 0): # TODO Can we get a reverse iterator over elements?
                    let c = buf[i]
                    let curw = c.graphwidth
                    if dispw + curw >= effw: break
                    dispw += curw
                    dispn += 1
                offset = pos + delta - dispn
                tty.write(goto(0))
                drawFrom(offset)
                tty.write(goto(dispw - dispend))
            else: tty.write("\e[" & $buf[pos..<pos + delta].graphwidth & "C")
        elif ndelta < 0:
            if dispn < buf.len and pos - delta < offset: # Going off the left end
                offset -= delta - (pos - offset)
                dispw = 0
                dispn = 0
                for c in buf[offset..^1]:
                    let curw = c.graphwidth
                    if dispw + curw > effw: break
                    dispw += curw
                    dispn += 1
                tty.write(goto(0))
                drawFrom(offset)
                tty.write(goto(0))
            else: tty.write("\e[" & $buf[pos - delta..<pos].graphwidth & "D")
        pos += ndelta
    proc reset(value: string) =
        buf = value.toRunes
        pos = 0
        offset = 0
        dispw = 0
        dispn = 0
        for c in buf:
            let curw = c.graphwidth
            if dispw + curw > effw: break
            dispw += curw
            dispn += 1
        tty.write(goto(-promptw) & prompt & repeat(' ', effw) & goto(0) & "\e[?25h\e[?1000l")
        drawFrom(0)
        move(buf.len)
        doCallback()
    proc histmove(by: int) =
        let oldidx = histidx
        var newidx = max(min(histidx + by, histedit.len - 1), 0)
        if oldidx == newidx: return
        histedit[histidx] = $buf
        histidx = newidx
        reset(histedit[histidx])
    reset(init)
    defer: tty.write("\e[?25l\e[?1000h")
    while true:
        let key = tty.readchar()
        case key:
        of '\x0d': return $buf # Enter
        of '\x7f': # Backspace
            if pos <= 0: continue
            move(-1)
            let rmwidth = buf[pos].graphwidth
            buf.delete(pos)
            dispw -= rmwidth
            dispn -= 1
            for c in buf[offset + dispn..^1]:
                let curw = c.graphwidth
                if dispw + curw > effw: break
                dispw += curw
                dispn += 1
            drawFrom(pos)
            doCallback()
        of '\x01': move(-pos) # ^A
        of '\x05': move(buf.len - pos) # ^E
        of '\x1b': # Escape
            let (cmd, args) = ctlseq(tty)
            case cmd
            of '\0': return ""
            of 'C': move(1) # Right
            of 'D': move(-1) # Left
            of 'A': histmove (-1) # Up
            of 'B': histmove(1) # Down
            of '~':
                if args == @[3]: # Delete
                    if pos >= buf.len: continue
                    let rmwidth = buf[pos].graphwidth
                    buf.delete(pos)
                    dispw -= rmwidth
                    dispn -= 1
                    drawFrom(pos)
                    doCallback()
                elif args == @[1]: move(-pos) # Home
                elif args == @[4]: move(buf.len - pos) # End
            else: discard
        else:
            var input = $key
            if (key.uint8 and 0x80) > 0.uint: # UTF-8 input
                var charlen = 0
                if (key.uint8 and 0xf8) == 0xf0: charlen = 3
                elif (key.uint8 and 0xf0) == 0xe0: charlen = 2
                elif (key.uint8 and 0xe0) == 0xc0: charlen = 1
                for i in 1..charlen: input &= tty.readchar()
            let c = input.runeAt(0)
            assert c.toUTF8.len == input.len
            buf.insert(c, pos)
            dispw += c.graphwidth
            dispn += 1
            while dispw + c.graphwidth > effw:
                assert buf.len >= offset + dispn
                dispw -= buf[offset + dispn - 1].graphwidth
                dispn -= 1
            if pos - offset < dispn: drawFrom(pos)
            move(1)
            doCallback()
