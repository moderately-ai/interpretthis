def f(): return 1
def g(): return 2
print(len({f, g, f}))
d = {f: "eff", g: "gee"}
print(d[f], d[g])
lams = [lambda: i for i in range(3)]
print(len(set(lams)))
print(len({lambda: 1, lambda: 2}))
print(f in {f, g})
h = f
print(h in {f})
funcs = {f, g}
funcs.add(f)
print(len(funcs))
print(hash(f) == hash(f))
print({f: 1}.get(f))
callables = {print: "builtin_print"} if False else {}
print(len(callables))
s = {f}
s.add(g)
s.discard(f)
print(len(s))
mapping = {}
for fn in [f, g, f, g]:
    mapping[fn] = mapping.get(fn, 0) + 1
print(sorted(mapping.values()))
