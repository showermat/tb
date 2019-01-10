import algorithm
import deques
import jsonvalue
import listformat

iterator getChildren(v: JsonValue, fwd: bool): (int, JsonValue) =
    let c = v.children
    if c != nil:
        for i, child in (if fwd: c else: c.reversed):
            let idx = if fwd: i else: c.len - i - 1
            yield (idx, child)

iterator dfs*(root: JsonValue, q: string, fwd: bool = true, start: seq[int]): seq[int] =
    var stack = newSeq[(seq[int], JsonValue)]()
    var cur = root
    stack &= (newSeq[int](), root)
    # Since we can start from an arbitrary position in the tree, begin by adding to the stack the parts of the tree that will not be fully covered
    let startPath = if fwd: start & -1 else: start # If we're iterating forward, we need to add all of start's children to the stack
    for length, elem in startPath.pairs:
        for idx, child in cur.getChildren(not fwd):
            if idx == elem:
                cur = child
                break
            let path = start[0..<length] & idx
            stack &= (path, child)
    # The rest is standard DFS
    while stack.len > 0:
        let (path, node) = stack.pop
        if node.content.contains(q): yield path
        for i, child in node.getChildren(not fwd):
            stack &= (path & i, child)
