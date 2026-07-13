# class_pattern_positional: PEP 634 class patterns with positional
# sub-patterns walk __match_args__. Pins lookup_match_args +
# subject_is_instance_of in match_stmt.rs.
class Point:
    __match_args__ = ("x", "y")

    def __init__(self, x, y):
        self.x = x
        self.y = y

p = Point(3, 4)
match p:
    case Point(0, 0):
        print("origin")
    case Point(x, 0):
        print(f"on x-axis at {x}")
    case Point(0, y):
        print(f"on y-axis at {y}")
    case Point(x, y):
        print(f"general ({x}, {y})")

# Origin via positional capture.
match Point(0, 0):
    case Point(x, y):
        print(f"x={x} y={y}")

# Negative case: wrong type doesn't match Point().
match "hello":
    case Point(x, y):
        print(f"matched Point({x}, {y})")
    case _:
        print("not a Point")
