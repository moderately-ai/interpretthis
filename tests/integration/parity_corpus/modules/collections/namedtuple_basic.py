# namedtuple synthesises a class with field-named attributes. Pins
# call_namedtuple_with_state's class registration. Iteration/len are
# covered in namedtuple_iteration.py; subscript in namedtuple_indexing.py.
import collections
Point = collections.namedtuple("Point", "x y")
p = Point(3, 4)
print(p.x)
print(p.y)
# _fields class attribute lists the fields.
print(Point._fields)
# Multiple fields with a list
Color = collections.namedtuple("Color", ["r", "g", "b"])
c = Color(255, 128, 0)
print(c.r, c.g, c.b)
print(Color._fields)
# PEP 634 class pattern uses __match_args__ which namedtuple auto-sets.
match p:
    case Point(0, 0):
        print("origin")
    case Point(x, y):
        print(f"({x}, {y})")
