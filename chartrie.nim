import tables

type Trie = ref object
    children: Table[char, Trie]
    action: proc()

proc newTrie*(): Trie =
    Trie(children: initTable[char, Trie](), action: nil)

proc register*(t: Trie, path: string, action: proc()) =
    if path == "":
        t.action = action
        return
    let child = t.children.mgetOrPut(path[0], newTrie())
    child.register(path[1..^1], action)

proc wait*(t: Trie, f: File, path: string = "") =
    if t.action != nil: t.action()
    if t.children.len == 0: return
    let next = readchar(f)
    # TODO One-second timeout
    if not (next in t.children): return
    t.children[next].wait(f, path & next)

template on*(t: Trie, paths: varargs[string], action: untyped) =
    for path in paths: t.register(path, proc() = action)
