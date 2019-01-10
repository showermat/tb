import jsonvalue
import tables
import strutils
import selectors
import util
import listnode
import chartrie
import prompt
import sets
import unicode
import times
import posix, os # Bug in Nim standard library?  Required to use selectors

let
    fgMuted = newColorSpec(244, 4, false)
    bgSelected = newColorSpec(238, 7, true)
    bgHighlight = newColorSpec(88, 3, true)

type DisplayList = ref object
    tty: File # File descriptor for interaction
    root: ListNode # Root node of the JSON tree
    sel: ListNode # Currently selected node
    width: int # Terminal width
    height: int # Canvas height; terminal height minus one for status line
    start: ListPos # Node and line at the top of the screen
    offset: int # Line number of the selected node (distance from start to sel)
    down: bool # The direction of the last movement
    lineno: int # Current scroll position, in lines
    query: string # The current search query
    lastclick: times.Time # Time of last click, for double-click detection
    numbuf: string # Buffer for numbers entered to prefix a command

proc newDisplayList*(val: JsonValue, tty: File): DisplayList =
    let size = tty.scrsize
    let root = newRootNode(val, size.w)
    root.toggle(size.w)
    return DisplayList(tty: tty, root: root, sel: root, width: size.w, height: size.h - 1, start: lpos(root, 0), offset: 0, down: true, lineno: 0, query: nil, lastclick: fromUnix(0), numbuf: "")

iterator items(l: DisplayList): ListNode =
    var n = l.root
    while n != nil:
        yield n
        n = n.next

proc last(l: DisplayList): ListNode =
    var cur = l.root
    while true:
        if cur.nextsib != nil: cur = cur.nextsib
        elif cur.next != nil: cur = cur.next
        else: return cur

proc getLine(l: DisplayList, cur: ListPos): (string, string) =
    cur.node.getLine(cur.line)

proc drawLine(l: DisplayList, line: int, cur: ListPos) =
    const debug = false
    l.tty.write("\e[" & $(line + 1) & ";0H\e[K")
    if cur.node == nil: return
    when debug:
        l.tty.write("\e[47m\e[30m\e[2K " & $line)
        l.tty.flushFile()
        sleep(50)
        l.tty.write("\e[m\r\e[2K")
    var (prefix, content) = l.getLine(cur)
    if l.query != nil:
        let points = cur.node.search(l.query, cur.line)
        if points.len > 0:
            var newcontent = ""
            var pointidx = 0
            var i = 0
            for c in content.runes:
                if pointidx < points.len and i == points[pointidx]:
                    if pointidx mod 2 == 0: newcontent &= startColor(bgHighlight)
                    elif l.sel == cur.node: newcontent &= startColor(bgSelected)
                    else: newcontent &= endBg
                    pointidx += 1
                newcontent &= c.toUTF8
                i += 1
            if points[^1] == i: newcontent &= endBg
            content = newcontent
    l.tty.write(
        colorPrint(fgMuted, prefix) &
        (if l.sel == cur.node: colorPrint(bgSelected, content) else: content)
    )

proc drawLines(l: DisplayList, first: int, last: int) =
    var cur = l.start.move(first, true)
    #debugmsg(l.tty, $first & ":" & $last)
    for i in first..<last:
        l.drawLine(i, cur)
        cur = cur.move(1, true)

proc drawLines(l: DisplayList, lines: (int, int)) = l.drawLines(lines[0], lines[1])

proc setquery(l: DisplayList, query: string) =
    l.query = if query == "": nil else: query
    var redraw = initTable[int, ListPos]()
    var cur = l.start.node
    var line = -l.start.line
    proc onscreen(i: int): bool = i >= 0 and i < l.height
    while line < l.height:
        for match in cur.matchLines:
            if onscreen(line + match): redraw[line + match] = lpos(cur, match)
        if l.query != nil:
            discard cur.search(l.query, 0)
            for match in cur.matchLines:
                if onscreen(line + match): redraw[line + match] = lpos(cur, match)
        line += cur.lines
        cur = cur.next
        if cur == nil: break
    for line, pos in redraw.pairs: l.drawline(line, pos)

proc selLines(l: DisplayList): (int, int) =
    if not l.down: return (l.offset, min(l.offset + l.sel.lines, l.height))
    else: return (max(l.offset - l.sel.lines + 1, 0), l.offset + 1)

proc statline(l: DisplayList) =
    proc writeat(n: int, s: string) =
        if s != "": l.tty.write("\e[" & $(l.height + 1) & ";" & $n & "H" & s)
    writeat(0, "\e[K")
    writeat(l.width - 8, $(l.lineno + l.offset))
    writeat(l.width - 16, $l.numbuf)

proc scroll(l: DisplayList, by: int): int =
    let oldSel = l.sel
    let newStart = l.start.move(by)
    let diff = if by > 0: l.start.distanceFwd(newStart) elif by < 0: -newStart.distanceFwd(l.start) else: 0
    let dist = diff.abs
    l.start = newStart
    l.offset -= diff
    if by > 0:
        while l.offset < 0:
            if not l.down:
                l.offset += l.sel.lines - 1
                l.down = true
            else:
                l.sel = l.sel.next
                l.offset += l.sel.lines
    else:
        while l.offset >= l.height:
            if l.down:
                l.offset -= l.sel.lines - 1
                l.down = false
            else:
                l.sel = l.sel.prev
                l.offset -= l.sel.lines
    #debugmsg($l.offset) # FIXME?
    if dist >= l.height: l.drawLines(0, l.height)
    elif diff != 0:
        if diff > 0:
            l.tty.write("\e[H\e[" & $dist & "M")
            l.drawLines(l.height - dist, l.height)
        elif diff < 0:
            l.tty.write("\e[H\e[" & $dist & "L")
            l.drawLines(0, dist)
        if l.sel != oldSel: l.drawLines(l.selLines)
    l.lineno += diff
    l.statline()
    return diff

proc select(l: DisplayList, newSel: ListNode): int =
    if newSel == nil: return
    let down = l.sel.isBefore(newSel)
    let oldLines = l.selLines
    let curPos = if l.down: l.sel.lines - 1 else: 0
    if down: l.offset += lpos(l.sel, curPos).distanceFwd(lpos(newSel, newSel.lines - 1))
    else: l.offset -= lpos(newSel, 0).distanceFwd(lpos(l.sel, curPos))
    l.down = down
    l.sel = newSel
    var scrollDist = 0
    if l.offset < 0: scrollDist = l.scroll(l.offset)
    elif l.offset >= l.height: scrollDist = l.scroll(l.offset - l.height + 1)
    else: l.statline()
    if oldLines[0] - scrollDist < l.height and oldLines[1] - scrollDist >= 0:
        l.drawLines(max(oldLines[0] - scrollDist, 0), min(oldLines[1] - scrollDist, l.height)) # Clear the old selection
    if scrollDist.abs < l.height:
        var selLines = l.selLines
        if scrollDist > 0: selLines = (min(selLines[0], l.height - scrollDist), min(selLines[1], l.height - scrollDist))
        elif scrollDist < 0: selLines = (max(selLines[0], -scrollDist), max(selLines[1], -scrollDist))
        if selLines[0] < selLines[1]: l.drawLines(selLines)
    return scrollDist

proc resize(l: DisplayList) =
    let size = l.tty.scrsize
    l.width = size.w
    l.height = size.h - 1
    for n in l.items: n.reformat(l.width)
    discard l.select(l.sel)
    l.drawLines(0, l.height)
    l.statline()

proc selpos(l: DisplayList, line: int) =
    discard l.select(l.start.move(line).node)

proc togglesel(l: DisplayList) =
    var maxEnd: int
    if l.sel.expanded: maxEnd = lpos(l.sel, 0).distanceFwd(lpos(nil, 0))
    l.sel.toggle(l.width)
    if l.sel.expanded: maxEnd = lpos(l.sel, 0).distanceFwd(lpos(nil, 0))
    l.drawLines(l.offset, min(l.height, l.offset + maxEnd))

proc searchnext(l: DisplayList, offset: int = 1) =
    if l.query == nil or offset == 0: return
    var matches = 0
    var respath: seq[int] = nil
    for path in l.sel.searchFrom(l.query, offset > 0):
        matches += 1
        if matches >= offset.abs:
            respath = path
            break
    if respath == nil: return
    var firstline = -1
    var n = l.root
    for i in respath:
        if n.expandable and not n.expanded:
            if firstline < 0: firstline = l.start.distanceFwd(lpos(n, 0))
            n.expand(l.width)
            if n.isBefore(l.sel):
                let newlines = lpos(n, 0).distanceFwd(lpos(n.nextsib, 0)) - 1
                if not n.isBefore(l.start.node): l.offset += newlines
                else: l.lineno += newlines
        n = n.children[i]
    #if firstline >= 0: l.drawLines(firstline, min(l.start.distanceFwd(lpos(nil, 0)), l.height))
    #discard l.select(n)
    var lastLine = min(l.start.distanceFwd(lpos(nil, 0)), l.height)
    let scrollDist = l.select(n)
    if firstLine >= 0 and scrollDist.abs < l.height:
        firstLine -= scrollDist
        lastLine -= scrollDist
        if firstLine < l.height and lastLine >= 0:
            l.drawLines(max(firstLine, 0), min(lastLine, l.height))

proc search(l: DisplayList, forward: bool) =
    let oldquery = l.query
    l.setquery(nil)
    proc incsearch(q: string) =
        l.setquery(q) # Optionally, also call searchnext, but always from the original position at the start of the search
    let res = prompt(l.tty, (l.height + 1, 1), l.width - 20, if forward: "/" else: "?", "", @[], incsearch)
    if res == nil: l.setquery(oldquery)
    l.searchnext(if forward: 1 else: -1)

proc click(l: DisplayList, y: int) =
    let now = getTime()
    l.selpos(y)
    if now - l.lastclick < 1:
        l.togglesel() # initDuration(milliseconds = 400)
        l.lastclick = fromUnix(0)
    else: l.lastclick = now

proc addnum(l: DisplayList, n: string) =
    if n == nil: l.numbuf = ""
    else:
        if n == "0" and l.numbuf.len == 0: return
        if l.numbuf.len >= 6: l.numbuf = l.numbuf[1..^1]
        l.numbuf &= n
    l.statline()

proc getnum(l: DisplayList): int =
    if l.numbuf == "": return 1
    return l.numbuf.parseInt

proc seek(l: DisplayList, rel: proc(n: ListNode): ListNode): ListNode =
    var ret = l.sel
    for i in 1..l.getnum:
        let next = rel(ret)
        if next == nil: break
        ret = next
    return ret

proc interactive*(l: DisplayList) =
    l.tty.scr_init()
    l.tty.scr_save()
    defer: l.tty.scr_restore()
    for n in l.items: n.reformat(l.width) # Necessary to refresh cached formatted string with new color palette from scr_init.  TODO Try to get rid of this
    l.drawLines(0, l.height)
    l.statline()
    var events = newSelector[int]()
    let infd = l.tty.getFileHandle.int
    events.registerHandle(infd, {Read}, 0)
    let winchfd = events.registerSignal(28, 0)
    let termfd = events.registerSignal(15, 0)
    var done = false
    let digits = @["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"]
    let keys = newTrie()
    keys.register(digits, proc(s: string) = l.addnum(s))
    keys.on(" "): l.togglesel()
    keys.on("w"):
        l.sel.recursiveExpand(l.width)
        l.drawLines(l.offset, min(l.height, l.offset + lpos(l.sel, 0).distanceFwd(lpos(nil, 0))))
    keys.on("\x0d"): l.sel.edit() # ^M
    keys.on("\x0c"): l.drawLines(0, l.height) # ^L
    keys.on("k"): discard l.select(l.seek(proc(n: ListNode): ListNode = n.prev)) # Ugh, this syntax is a bit heavy
    keys.on("j"): discard l.select(l.seek(proc(n: ListNode): ListNode = n.next))
    keys.on("J"): discard l.select(l.seek(proc(n: ListNode): ListNode = n.nextsib))
    keys.on("K"): discard l.select(l.seek(proc(n: ListNode): ListNode = n.prevsib))
    keys.on("p"): discard l.select(l.seek(proc(n: ListNode): ListNode = n.parent))
    keys.on("g"): discard l.select(l.root)
    keys.on("G"): discard l.select(l.last)
    keys.on("H"): l.selpos(0)
    keys.on("M"): l.selpos(l.height div 2)
    keys.on("L"): l.selpos(l.height - 1)
    keys.on("zz"): discard l.scroll(l.offset - l.height div 2)
    keys.on("\x05"): discard l.scroll(l.getnum) # ^E
    keys.on("\x19"): discard l.scroll(-l.getnum) # ^Y
    keys.on("\x02"): discard l.scroll(-l.getnum * l.height) # ^B
    keys.on("\x06"): discard l.scroll(l.getnum * l.height) # ^F
    keys.on("\x04"): discard l.scroll(l.getnum * l.height div 2) # ^D
    keys.on("\x15"): discard l.scroll(-l.getnum * l.height div 2) # ^U
    keys.on("/"): l.search(true)
    keys.on("?"): l.search(false)
    keys.on("n"): l.searchnext(l.getnum)
    keys.on("N"): l.searchnext(-l.getnum)
    keys.on("c"): l.setquery(nil)
    keys.on("q", "\x03"): done = true # ^C
    keys.on("\x1a"): discard # ^Z TODO
    keys.on("\e"):
        let (cmd, args) = ctlseq(l.tty)
        case cmd
        of 'A': discard l.select(l.seek(proc(n: ListNode): ListNode = n.prev)) # Up
        of 'B': discard l.select(l.seek(proc(n: ListNode): ListNode = n.next)) # Down
        of '~':
            if args.len != 1: discard
            else:
                case args[0]
                of 1: discard l.select(l.root) # Home
                of 4: discard l.select(l.last) # End
                of 5: discard l.scroll(-l.getnum * l.height) # PgUp
                of 6: discard l.scroll(l.getnum * l.height) # PgDown
                else: discard
        of 'M':
            if args.len != 3: discard
            elif args[2] > l.height: discard
            else:
                case args[0]
                of 0: l.click(args[2] - 1) # Button 1 down
                of 64: discard l.scroll(-4) # Scroll up
                of 65: discard l.scroll(4) # Scroll down
                else: discard
        else: discard
    while not done:
        for event in events.select(-1):
            if event.fd == winchfd: l.resize
            elif event.fd == termfd: done = true
            elif event.fd == infd:
                let cmd = keys.wait(l.tty)
                if not (cmd in digits): l.addnum(nil)
