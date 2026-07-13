# class_pattern_keyword: keyword sub-patterns name the attribute
# directly, bypassing __match_args__. Pins the kwd_attrs / kwd_patterns
# branch in match_class.
class Rectangle:
    def __init__(self, width, height):
        self.width = width
        self.height = height

r = Rectangle(10, 20)
match r:
    case Rectangle(width=10, height=20):
        print("10x20")
    case Rectangle(width=w, height=h):
        print(f"{w}x{h}")

# Mixed: just width specified, height bound to a name.
match r:
    case Rectangle(width=10, height=h):
        print(f"width 10 with height {h}")

# Negative: wrong width doesn't match.
match Rectangle(5, 5):
    case Rectangle(width=10):
        print("matched 10")
    case Rectangle(width=w):
        print(f"width={w}")
