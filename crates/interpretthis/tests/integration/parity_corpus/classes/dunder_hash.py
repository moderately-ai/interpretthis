# Pins: user-class __hash__ dispatches when an instance is used as a
# dict key or set member. Without it, two value-equal instances hash
# differently and collide as separate entries.
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

s = {Key('a'), Key('a'), Key('b')}
print(len(s))
