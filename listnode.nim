import listformat
import listvalue
import jsonvalue
import unicode
import algorithm
import strutils
import sets
import util

type ListNode* = ref object
    prev*, next*: ListNode # Last and next items in the visual display
    prevsib*, nextsib*: ListNode # Last and next items at equal or higher level
    parent*: ListNode # Parent node
    children*: seq[ListNode] # Visible child nodes
    expanded*: bool # Whether this node is expanded to show children
    last: bool # Whether this is the last child of its parent
    value: JsonValue # The backing value
    cache: tuple[placeholder: Preformatted, content: Preformatted]
    search: tuple[query: string, res: seq[((int, int), (int, int))]]

type ListPos* = object
    node*: ListNode
    line*: int

proc lpos*(node: ListNode, line: int): ListPos =
    return ListPos(node: node, line: line)

proc depth(n: ListNode): int =
    if n.parent == nil: 0
    else: n.parent.depth + 1

proc pathTo(n: ListNode): seq[int] =
    if n.parent == nil: return @[]
    return n.parent.pathTo & n.value.index

proc root(n: ListNode): ListNode =
    if n.parent == nil: return n
    return n.parent.root

proc ins(after: ListNode, n: ListNode) =
    # FIXME Sibling links will be updated incorrectly if the node on either side is deeper in the tree than the one being inserted.
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
    return n.parent.parentPrefix & (if n.last: "└── " else: "├── ")

# The following three functions, while more elegantly written recursively, lead to stack overflows in large lists
proc distanceFwd*(fro: ListPos, to: ListPos): int =
    var cur = fro
    var ret = 0
    while cur.node != to.node:
        if cur.node == nil: return -1
        ret += cur.node.cache.content.len - cur.line
        cur = lpos(cur.node.next, 0)
    return ret + to.line - cur.line

proc fwd(fro: ListPos, n: int, unsafe: bool): ListPos =
    if fro.node == nil: return lpos(fro.node, 0)
    var cur = fro
    var remain = n
    while remain >= cur.node.cache.content.len - cur.line:
        if cur.node.next == nil:
            if unsafe: return lpos(nil, 0)
            else: return lpos(cur.node, cur.node.cache.content.len - 1)
        remain -= cur.node.cache.content.len - cur.line
        cur = lpos(cur.node.next, 0)
    return lpos(cur.node, cur.line + remain)

proc bwd(fro: ListPos, n: int, unsafe: bool): ListPos =
    var cur = fro
    var remain = n
    while remain > cur.line:
        if cur.node.prev == nil:
            if unsafe: return lpos(nil, 0)
            else: return lpos(cur.node, 0)
        remain -= cur.line + 1
        cur = lpos(cur.node.prev, cur.node.prev.cache.content.len - 1)
    return lpos(cur.node, cur.line - remain)

proc move*(fro: ListPos, n: int, unsafe: bool = false): ListPos =
    if n < 0: return fro.bwd(-n, unsafe)
    if n > 0: return fro.fwd(n, unsafe)
    return fro

proc reformat*(n: ListNode, screenwidth: int) =
    n.cache.content = format(n.value.content, screenwidth - n.depth * 4)
    n.cache.placeholder = format(n.value.placeholder, screenwidth - n.depth * 4)

proc newListNode(parent: ListNode, val: JsonValue, width: int): ListNode =
    let ret = ListNode(prev: nil, next: nil, prevsib: nil, nextsib: nil, parent: parent, last: false, children: @[], value: val, cache: (nil, nil), expanded: false, search: (nil, @[]))
    ret.reformat(width)
    return ret

proc newRootNode*(val: JsonValue, width: int): ListNode =
    let ret = newListNode(nil, val, width)
    ret.last = true
    return ret

proc expandable*(n: ListNode): bool =
    n.value.children.len > 0

proc expand*(n: ListNode, w: int) =
    if n.expanded: return
    let children = n.value.children
    if children == nil: return
    var cur = n
    for child in children:
        let child = newListNode(n, child, w)
        n.children &= child
        cur.ins(child)
        cur = child
    if n.children.len > 0: n.children[^1].last = true
    n.expanded = true

proc collapse*(n: ListNode) =
    if not n.expanded: return
    n.next = n.nextsib
    if n.next != nil: n.next.prev = n
    n.children = @[]
    n.expanded = false

proc lines*(n: ListNode): int =
    n.cache.content.len
    #case n.expanded # TODO Why is this wrong?
    #of true: n.cache.placeholder.len
    #of false: n.cache.content.len

proc toggle*(n: ListNode, w: int) =
    if n.expanded: n.collapse
    else: n.expand(w)

proc recursiveExpand*(n: ListNode, w: int) =
    if n.value.children == nil: return
    if not n.expanded: n.toggle(w)
    for child in n.children: child.recursiveExpand(w)

proc getLine*(n: ListNode, line: int): (string, string) =
    let value = case n.expanded
    of true: n.cache.placeholder[line]
    of false: n.cache.content[line]
    let prefix = case line
    of 0: n.prefix()
    else: n.parentPrefix()
    return (prefix, value)

proc search*(n: ListNode, q: string, line: int): seq[int] =
    let fmt = case n.expanded
    of true: n.cache.placeholder
    of false: n.cache.content
    if q != n.search.query: # TODO Need to refresh if expanded state changes?
        n.search.query = q
        n.search.res = fmt.search(q)
    var ret: seq[int] = @[]
    for res in n.search.res:
        if (res[0][0] < line and res[1][0] < line) or (res[0][0] > line and res[1][0] > line): continue
        var start = res[0][1]
        var stop = res[1][1]
        if res[0][0] < line: start = 0
        if res[1][0] > line: stop = fmt[line].runeLen
        if ret.len > 0 and ret[^1] == start: ret[^1] = stop
        else: ret &= @[start, stop]
    return ret

iterator searchFrom*(n: ListNode, q: string, fwd: bool = true): seq[int] =
    var res = false
    while true:
        for path in n.root.value.dfs(q, fwd, n.pathTo):
            res = true
            yield path
        if not res: break

proc isBefore*(a: ListNode, b: ListNode): bool =
    let path1 = a.pathTo
    let path2 = b.pathTo
    for i in 0..max(path1.len, path2.len):
        if path2.len <= i: return false
        if path1.len <= i: return true
        if path1[i] > path2[i]: return false
        if path1[i] < path2[i]: return true

proc matchLines*(n: ListNode): HashSet[int] =
    if n.search.query == nil: return initSet[int]()
    var ret = initSet[int]()
    for res in n.search.res:
        for i in res[0][0]..res[1][0]: ret.incl(i)
    return ret

proc edit*(n: ListNode) =
    discard # TODO
