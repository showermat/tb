import json
import tables
import util
import strutils
import unicode
import selectors
import chartrie
import posix, os # Bug in Nim standard library?  Required to use selectors

const
    fgString = newColorSet(154, 2, false)
    fgKeyword = newColorSet(214, 1, false)
    fgKey = newColorSet(183, 5, false)
    fgMuted = newColorSet(244, 4, false)
    bgHighlight = newColorSet(238, 7, true)

type ListNode = ref object
    prev, next: ListNode # Last and next items in the visual display
    prevsib, nextsib: ListNode # Last and next items at equal or higher level
    parent: ListNode # Parent node
    last: bool # Whether this is the last child of its parent
    children: seq[ListNode] # Visible child nodes
    key: string # Key, if parent is an object
    value: JsonNode # The actual JSON datum
    formatted: seq[string] # The value, preformatted for display
    expanded: bool # Whether this node is expanded to show children

type ListPos = object
    node: ListNode
    line: int

proc lpos(node: ListNode, line: int): ListPos =
    return ListPos(node: node, line: line)

proc depth(n: ListNode): int =
    if n.parent == nil: 0
    else: n.parent.depth + 1

proc expandable(n: ListNode): bool =
    case n.value.kind
    of JArray, JObject: return true
    else: return false

proc ins(after: ListNode, n: ListNode) =
    # Sibling links will be updated incorrectly if the node on either side is deeper in the tree than the one being inserted.
    if after.next != nil:
        after.next.prev = n
        n.next = after.next
        if after.next.parent == n.parent: after.next.prevsib = n
    after.next = n
    n.prev = after
    n.nextsib = n.next
    n.prevsib = n.prev
    if after != n.parent: after.nextsib = n

proc parentPrefix(n: ListNode): string =
    if n.parent == nil: return ""
    if n.last: return n.parent.parentPrefix & "    "
    else: return n.parent.parentPrefix & "│   "

proc prefix(n: ListNode): string =
    if n.parent == nil: return ""
    return color(fgMuted, n.parent.parentPrefix & (if n.last: "└── " else: "├── "))

proc distanceFwd(fro: ListPos, to: ListPos): int =
    if fro.node == to.node: return to.line - fro.line
    if fro.node == nil: return -1
    let next = lpos(fro.node.next, 0).distanceFwd(to)
    if next < 0: return next
    return next + fro.node.formatted.len - fro.line

proc fwd(fro: ListPos, n: int, unsafe: bool = false): ListPos =
    if fro.node == nil or fro.line + n < fro.node.formatted.len: return lpos(fro.node, fro.line + n)
    if fro.node.next == nil:
        if unsafe: return lpos(nil, 0)
        else: return lpos(fro.node, fro.node.formatted.len - 1)
    return lpos(fro.node.next, 0).fwd(n - fro.node.formatted.len + fro.line, unsafe)

proc bwd(fro: ListPos, n: int, unsafe: bool = false): ListPos =
    if fro.node == nil or fro.line - n >= 0: return lpos(fro.node, fro.line - n)
    if fro.node.prev == nil:
        if unsafe: return lpos(nil, 0)
        else: return lpos(fro.node, 0)
    return lpos(fro.node.prev, fro.node.prev.formatted.len - 1).bwd(n - fro.line - 1, unsafe)

proc move(fro: ListPos, n: int): ListPos =
    if n < 0: return fro.bwd(-n)
    if n > 0: return fro.fwd(n)
    return fro

proc fmtkey(n: ListNode): string =
    if n.parent == nil:
        if n.expanded: return color(fgMuted, "•")
        else: return ""
    let ret = case n.parent.value.kind
    of JObject: color(fgKey, n.key)
    of JArray: color(fgMuted, n.key)
    else: "" #assert(false)
    return ret & (if n.expanded: "" else: color(fgMuted, ": "))

proc fmtstr(s: string, w: int): seq[string] =
    var ret: seq[string] = @[]
    var cur = startColor(fgString)
    var cnt = 0
    for c in s.runes:
        case c.int32
        of 10: # \n
            ret &= cur & endFg
            cur = startColor(fgString)
            cnt = 0
        of 9: # \t
            if cnt >= w - 4:
                ret &= cur & endFg
                cur = ""
                cnt = 0
            cur &= "    "
            cnt += 4
        of 0..8, 11..31, 127:
            if cnt >= w - 2:
                ret &= cur & endFg
                cur = ""
                cnt = 0
            cur &= startColor(fgKeyword) & "^" & ((c.ord + 64) mod 128).chr & startColor(fgString)
            cnt += 2
        else:
            let cw = wcwidth(c.int32)
            if cnt + cw > w:
                ret &= cur & endFg
                cur = startColor(fgString)
                cnt = 0
            cur &= c.toUTF8
            cnt += max(cw, 0)
    return ret & (cur & endFg)

proc fmtval(n: ListNode, w: int): seq[string] =
    case n.value.kind
    of JInt: @[color(fgKeyword, $n.value.num)]
    of JFloat: @[color(fgKeyword, $n.value.fnum)]
    of JBool: @[color(fgKeyword, $n.value.bval)]
    of JNull: @[color(fgKeyword, "null")]
    of JString: fmtstr(n.value.str, w - (n.depth * 4 + n.key.len + 2))
    of JObject: @[color(fgKeyword, if n.value.fields.len == 0: "{ }" else: "{...}")]
    of JArray: @[color(fgKeyword, if n.value.elems.len == 0: "[ ]" else: "[...]")]

proc reformat(n: ListNode, w: int) = n.formatted = n.fmtval(w)

proc newListNode(parent: ListNode, key: string, val: JsonNode, width: int): ListNode =
    let ret = ListNode(prev: nil, next: nil, prevsib: nil, nextsib: nil, parent: parent, last: false, children: @[], key: key, value: val, formatted: nil, expanded: false)
    ret.reformat(width)
    return ret

proc newRootNode(val: JsonNode, width: int): ListNode =
    let ret = newListNode(nil, nil, val, width)
    ret.last = true
    return ret

proc toggle(n: ListNode, w: int) =
    if n.expanded:
        n.next = n.nextsib
        if n.next != nil: n.next.prev = n
        n.children = @[]
        n.expanded = false
    elif n.expandable:
        var cur = n
        proc addchild(k: string, v: JsonNode) =
            let child = newListNode(n, k, v, w)
            n.children &= child
            cur.ins(child)
            cur = child
        case n.value.kind
        of JObject:
            for k, v in n.value.fields: addchild(k, v)
        of JArray:
            for i, v in n.value.elems: addchild($i, v)
        else: assert(false)
        if n.children.len > 0: n.children[^1].last = true
        n.expanded = true
    else: discard

proc recursiveExpand(n: ListNode, w: int) =
    if not n.expandable: return
    if not n.expanded: n.toggle(w)
    for child in n.children: child.recursiveExpand(w)

proc edit(n: ListNode) =
    discard

################################################################################

type DisplayList = ref object
    tty: File # File descriptor for interaction
    doc: JsonNode # JSON tree object
    root: ListNode # Root node of the JSON tree
    sel: ListNode # Currently selected node
    width: int # Terminal width
    height: int # Terminal height
    start: ListPos # Node and line at the top of the screen
    offset: int # Line number of the selected node (distance from start to sel)
    #selline: int # Line number within the selected node of our virtual cursor
    down: bool # The direction of the last movement

proc newDisplayList*(node: JsonNode, tty: File): DisplayList =
    let size = tty.scrsize
    let root = newRootNode(node, size.w)
    root.toggle(size.w)
    return DisplayList(tty: tty, doc: node, root: root, sel: root, width: size.w, height: size.h, start: lpos(root, 0), offset: 0, down: true)

iterator items(l: DisplayList): ListNode =
    var n = l.root
    while n != nil:
        yield n
        n = n.next

proc last(l: DisplayList): ListNode =
    var cur = l.sel
    if cur == nil: return cur
    while true:
        if cur.nextsib != nil: cur = cur.nextsib
        elif cur.next != nil: cur = cur.next
        else: return cur

proc drawLines(l: DisplayList, first: int, last: int) =
    var cur = l.start.fwd(first, true)
    for i in first..<last:
        l.tty.write("\e[" & $(i + 1) & ";0H\e[K")
        #l.tty.write("\e[47m\e[30m\e[2K " & $i)
        #l.tty.flushFile()
        #sleep(50)
        #l.tty.write("\e[m\r\e[2K")
        if cur.node == nil: continue
        let displayKey = case cur.line
        of 0: cur.node.fmtkey
        else: " ".repeat(cur.node.key.len + 2)
        let content = case cur.node.expanded
        of true: ""
        else: cur.node.formatted[cur.line]
        l.tty.write(
            color(fgMuted, if cur.line == 0: cur.node.prefix else: cur.node.parentPrefix) &
            (if l.sel == cur.node: color(bgHighlight, displayKey & content) else: displayKey & content)
        )
        cur = cur.fwd(1, true)

proc drawLines(l: DisplayList, lines: (int, int)) = l.drawLines(lines[0], lines[1])

proc selLines(l: DisplayList): (int, int) =
    if l.down: return (l.offset, min(l.offset + l.sel.formatted.len, l.height))
    else: return (max(l.offset - l.sel.formatted.len + 1, 0), l.offset + 1)

proc resize(l: DisplayList) =
    let size = l.tty.scrsize
    l.width = size.w
    l.height = size.h
    for n in l: n.reformat(l.width)
    l.drawLines(0, l.height)

proc scroll(l: DisplayList, by: int) =
    let oldSel = l.sel
    let newStart = l.start.move(by)
    let diff = if by > 0: l.start.distanceFwd(newStart) elif by < 0: -newStart.distanceFwd(l.start) else: 0
    let dist = diff.abs
    l.start = newStart
    l.offset -= diff
    if by > 0:
        while l.offset < 0:
            if l.down:
                l.offset += l.sel.formatted.len - 1
                l.down = false
            else:
                l.sel = l.sel.next
                l.offset += l.sel.formatted.len
    else:
        while l.offset >= l.height:
            if not l.down:
                l.offset -= l.sel.formatted.len - 1
                l.down = true
            else:
                l.sel = l.sel.prev
                l.offset -= l.sel.formatted.len
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

proc select(l: DisplayList, newSel: ListNode, down: bool) =
    if newSel == nil or newSel == l.sel: return
    let oldLines = l.selLines
    let curPos = if l.down: 0 else: l.sel.formatted.len - 1
    if down: l.offset += lpos(l.sel, curPos).distanceFwd(lpos(newSel, 0))
    else: l.offset -= lpos(newSel, newSel.formatted.len - 1).distanceFwd(lpos(l.sel, curPos))
    l.down = down
    l.sel = newSel
    l.drawLines(oldLines) # TODO Don't draw this if `dist >= l.height` in `scroll` above
    if l.offset < 0: l.scroll(l.offset)
    elif l.offset >= l.height: l.scroll(l.offset - l.height + 1)
    else: l.drawLines(l.selLines)

proc selpos(l: DisplayList, line: int) =
    l.select(l.start.fwd(line).node, if line > l.offset: true elif line < l.offset: false else: l.down)

proc interactive*(l: DisplayList) =
    l.tty.scr_init()
    l.tty.scr_save()
    defer: l.tty.scr_restore()
    l.drawLines(0, l.height)
    var events = newSelector[int]()
    let infd = l.tty.getFileHandle.int
    events.registerHandle(infd, {Read}, 0)
    let winchfd = events.registerSignal(28, 0)
    let termfd = events.registerSignal(15, 0)
    var done = false
    let keys = newTrie()
    keys.on(" "):
        var maxEnd: int
        if l.sel.expanded: maxEnd = lpos(l.sel, 0).distanceFwd(lpos(nil, 0))
        l.sel.toggle(l.width)
        if l.sel.expanded: maxEnd = lpos(l.sel, 0).distanceFwd(lpos(nil, 0))
        l.drawLines(l.offset, min(l.height, l.offset + maxEnd))
    keys.on("w"):
        l.sel.recursiveExpand(l.width)
        l.drawLines(l.offset, min(l.height, l.offset + lpos(l.sel, 0).distanceFwd(lpos(nil, 0))))
    #keys.on("\x0D"): l.sel.edit # ^M
    keys.on("\x0c"): l.drawLines(0, l.height) # ^L
    keys.on("j"): l.select(l.sel.next, true)
    keys.on("k"): l.select(l.sel.prev, false)
    keys.on("J"): l.select(l.sel.nextsib, true)
    keys.on("K"): l.select(l.sel.prevsib, false)
    keys.on("p"): l.select(l.sel.parent, false)
    keys.on("g"): l.select(l.root, false)
    keys.on("G"): l.select(l.last, true)
    keys.on("H"): l.selpos(0)
    keys.on("M"): l.selpos(l.height div 2)
    keys.on("L"): l.selpos(l.height - 1)
    keys.on("\x05"): l.scroll(1) # ^E
    keys.on("\x19"): l.scroll(-1) # ^Y
    keys.on("\x06"): l.scroll(l.height) # ^F
    keys.on("\x02"): l.scroll(-l.height) # ^B
    keys.on("\x04"): l.scroll(l.height div 2) # ^D
    keys.on("\x15"): l.scroll(-l.height div 2) # ^U
    keys.on("\x1b[A"): l.select(l.sel.prev, false) # Up
    keys.on("\x1b[B"): l.select(l.sel.next, true) # Down
    keys.on("\x1b[5~"): l.scroll(-l.height) # PgUp
    keys.on("\x1b[6~"): l.scroll(l.height) # PgDn
    keys.on("\x1b[1~"): l.select(l.root, false) # Home
    keys.on("\x1b[4~"): l.select(l.last, true) # End
    keys.on("q", "\x03"): done = true
    while not done:
        for event in events.select(-1):
            if event.fd == winchfd: l.resize
            elif event.fd == termfd: done = true
            elif event.fd == infd: keys.wait(l.tty)
