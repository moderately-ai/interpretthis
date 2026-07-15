def check(cmd):
    match cmd.split():
        case ["go", direction]:
            return f"going {direction}"
        case ["go", *rest]:
            return f"go where? {rest}"
        case [action]:
            return f"just {action}"
        case []:
            return "nothing"
    return "unknown"
print(check("go north"))
print(check("jump"))
print(check(""))
def describe_point(point):
    match point:
        case (0, 0):
            return "origin"
        case (x, 0):
            return f"x-axis at {x}"
        case (0, y):
            return f"y-axis at {y}"
        case (x, y):
            return f"point {x},{y}"
print(describe_point((0, 0)))
print(describe_point((5, 0)))
print(describe_point((3, 4)))
def handle(val):
    match val:
        case int() | float() if val > 0:
            return "positive number"
        case int() | float():
            return "non-positive number"
        case str():
            return "string"
        case _:
            return "other"
print(handle(5))
print(handle(-3))
print(handle("hi"))
print(handle([1]))
