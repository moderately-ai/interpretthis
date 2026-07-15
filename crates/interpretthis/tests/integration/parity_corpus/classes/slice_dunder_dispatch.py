# Slice subscripting dispatches to a user class's __getitem__/__setitem__/
# __delitem__ with a slice object, and a slice object used as a computed index
# on a builtin list does slice get/set/del.
class Vec:
    def __init__(self, data):
        self.data = list(data)
    def __getitem__(self, i):
        return Vec(self.data[i]) if isinstance(i, slice) else self.data[i]
    def __setitem__(self, i, v):
        self.data[i] = v
    def __delitem__(self, i):
        del self.data[i]
    def __repr__(self):
        return f"Vec({self.data})"

v = Vec([1, 2, 3, 4, 5])
print(v[1:4], v[::2], v[::-1])
v[1:3] = [20, 30, 40]
print(v)
del v[::2]
print(v)

# Computed slice object as a subscript on a plain list.
s = slice(1, 4)
lst = [0, 1, 2, 3, 4, 5]
print(lst[s], lst[slice(None, None, 2)])
lst[s] = [10, 20]
print(lst)
del lst[slice(0, None, 2)]
print(lst)
