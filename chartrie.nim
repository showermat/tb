import tables

type
    TrieNode = ref object
        children: Table[char, TrieNode]
        action: proc(s: string)
        owner: Trie

    Trie* = ref object
        root: TrieNode
        default: proc(s: string)

proc assign(t: TrieNode, path: string, action: proc(s: string)) =
    if path == "":
        t.action = action
        return
    let child = t.children.mgetOrPut(path[0], TrieNode(children: initTable[char, TrieNode](), action: nil, owner: t.owner))
    child.assign(path[1..^1], action)

proc wait(t: TrieNode, f: File, path: string): string =
    if t.action != nil: t.action(path)
    if t.children.len == 0:
        if t.action == nil: t.owner.default(path)
        return path
    let next = readchar(f)
    # TODO One-second timeout
    if next in t.children: return t.children[next].wait(f, path & next)
    else:
        t.owner.default(path & next)
        return path & next

proc newTrie*(): Trie =
    var ret = Trie(root: nil, default: proc(s: string) = discard)
    let root = TrieNode(children: initTable[char, TrieNode](), action: nil, owner: ret)
    ret.root = root
    return ret

proc register*(t: Trie, paths: openarray[string], action: proc(s: string)) =
    for path in paths: t.root.assign(path, action)

proc wait*(t: Trie, f: File, path: string = ""): string =
    return t.root.wait(f, path)

proc registerDefault*(t: Trie, action: proc(s: string)) =
    t.default = action

template on*(t: Trie, paths: varargs[string], action: untyped) =
    t.register(paths, proc(s: string) = action)
