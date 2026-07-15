print(type(type("E", (), {})()).__name__)


def make():
    return type("F", (), {"v": 1})


print(make()().v)
classes = [type("G", (), {"n": 7}), type("H", (), {"n": 8})]
print(classes[0]().n, classes[1]().n)
factory = type
print(factory("I", (), {"x": 5})().x)
Widget = type("Widget", (), {"render": lambda self: "widget"})
print(Widget().render())
print((type("J", (), {"z": 9}))().z)


# Calling a class held in a dict / returned from a call.
registry = {"point": type("Point", (), {"dims": 2})}
print(registry["point"]().dims)


class Base:
    kind = "base"


print((lambda: Base)()().kind)
