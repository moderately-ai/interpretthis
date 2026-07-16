from collections import UserDict, UserList, UserString


# UserDict — subclass overriding __setitem__ and delegating via super().
class MyDict(UserDict):
    def __setitem__(self, key, value):
        super().__setitem__(key.upper(), value)


d = MyDict()
d["abc"] = 1
print(d["ABC"])
print(dict(d.data))

ud = UserDict({"a": 1, "b": 2})
print(sorted(ud.keys()))
print(ud.get("a"), ud.get("z", "default"))
print(len(ud))
ud["c"] = 3
print(sorted(ud.items()))
print(ud.pop("a"))
print(sorted(ud.data.items()))
print(UserDict(a=1, b=2).data == {"a": 1, "b": 2})
print(UserDict({"a": 1}) == {"a": 1})
print("b" in ud)


# UserList — subclass overriding append; operators return the same class.
class CountingList(UserList):
    def append(self, item):
        print(f"appending {item}")
        super().append(item)


cl = CountingList([1, 2, 3])
cl.append(4)
print(cl.data, len(cl), cl[0])
print(cl[1:3].data, type(cl[1:3]).__name__)

l2 = UserList([3, 1, 2])
l2.sort()
print(l2.data)
print((l2 + [4, 5]).data)
print((l2 * 2).data)
print(l2.count(1), l2.index(2))
l2.extend([9, 8])
print(l2.data)
print(2 in l2, 99 in l2)
print(UserList([1, 2, 3]) == [1, 2, 3])
print([x for x in UserList([1, 2, 3])])


# UserString — subclass and delegated string methods.
class UpperString(UserString):
    def __init__(self, s):
        super().__init__(str(s).upper())


us = UpperString("hello")
print(us, us.data, len(us))

s2 = UserString("world")
print(s2.upper(), s2 + "!", s2 * 2)
print("wor" in s2)
print(UserString("  hi  ").strip())
print(UserString("a,b,c").split(","))
print(UserString("hello").replace("l", "L"))
print(UserString("hello").startswith("he"), UserString("hello").find("l"))
print(UserString("hello world").title())
print(UserString("world") == "world")
print(UserString("abc") < UserString("abd"))
print(UserString("hi")[0], type(UserString("hi")[0]).__name__)

# Aliased import binds only the alias.
from collections import UserDict as UD

print(UD({"x": 1})["x"])
