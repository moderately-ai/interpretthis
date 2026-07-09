# Pins: user `__hash__` that hashes a string field; dict lookup by equal key.
# Does not print raw hash integers (CPython hash randomization / algorithm
# differs from our i64 path) — only observable equality/lookup behaviour.
class Key:
    def __init__(self, name):
        self.name = name
    def __eq__(self, other):
        return isinstance(other, Key) and self.name == other.name
    def __hash__(self):
        return hash(self.name)

d = {Key('alpha'): 1}
print(d[Key('alpha')])
print(Key('alpha') in d)
print(Key('beta') in d)
# hash() on a user instance must invoke __hash__ (returns int, comparable).
h1 = hash(Key('x'))
h2 = hash(Key('x'))
print(h1 == h2)
print(isinstance(h1, int))
