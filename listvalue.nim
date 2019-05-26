import algorithm
import deques
import jsonvalue
import listformat

iterator getChildren(v: JsonValue, fwd: bool): (int, JsonValue) =
    if v.expandable:
        let c = v.children
        for i, child in (if fwd: c else: c.reversed):
            let idx = if fwd: i else: c.len - i - 1
            yield (idx, child)

iterator dfsFwd(root: JsonValue, q: string, start: seq[int]): seq[int] =
    var stack = newSeq[(seq[int], JsonValue)]()
    var cur = root
    stack &= (newSeq[int](), root)
    for length, elem in (start & -1).pairs:
        for idx, child in cur.getChildren(false):
            if idx == elem:
                cur = child
                break
            let path = start[0..<length] & idx
            stack &= (path, child)
    while stack.len > 0:
        let (path, node) = stack.pop
        if node.content.contains(q): yield path
        for i, child in node.getChildren(false):
            stack &= (path & i, child)

iterator dfsBwd(root: JsonValue, q: string, start: seq[int]): seq[int] =
    var stack = newSeq[(seq[int], JsonValue, bool)]()
    var cur = root
    stack &= (newSeq[int](), root, true)
    for length, elem in start.pairs:
        for idx, child in cur.getChildren(true):
            if idx == elem:
                if length < start.len - 1: stack &= (start[0..length], child, false)
                cur = child
                break
            let path = start[0..<length] & idx
            stack &= (path, child, true)
    while stack.len > 0:
        let (path, node, pushChildren) = stack.pop
        if pushChildren and node.expandable:
            stack &= (path, node, false)
            for i, child in node.getChildren(true):
                stack &= (path & i, child, true)
        elif node.content.contains(q): yield path

iterator dfs*(root: JsonValue, q: string, fwd: bool, start: seq[int]): seq[int] =
    if fwd:
        for x in root.dfsFwd(q, start): yield(x)
    else:
        for x in root.dfsBwd(q, start): yield(x)
