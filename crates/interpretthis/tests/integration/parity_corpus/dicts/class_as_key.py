class A: pass
class B: pass
d = {A: "a", B: "b"}
print(d[A], d[B])
print(A in {A, B}, str in {int, str, A})
print(len({A, A, B}))
print({int: 1, str: 2}[int])
registry = {}
registry[A] = [1, 2]
registry[A].append(3)
print(registry[A])
print(hash(A) == hash(A))
print(A == A, A != B)
cache = {ValueError: "ve", TypeError: "te"}
print(cache[ValueError])
print(frozenset([A, B, A]) == frozenset([B, A]))
