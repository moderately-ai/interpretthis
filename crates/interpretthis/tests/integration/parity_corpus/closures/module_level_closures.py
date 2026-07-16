funcs = [lambda x: x + i for i in range(3)]
print([f(10) for f in funcs])
adders = []
for i in range(3):
    adders.append(lambda: i)
print([f() for f in adders])
g = 1
f = lambda: g
g = 2
print(f())
squares = {i: (lambda: i * i) for i in range(3)}
print([squares[k]() for k in sorted(squares)])
handlers = []
for name in ["a", "b", "c"]:
    handlers.append(lambda: name)
print([h() for h in handlers])
callbacks = [lambda x, n=i: x * n for i in range(1, 4)]
print([c(10) for c in callbacks])
