# float() accepts underscore digit separators, but only between two digits.
def show(f):
    try:
        print(repr(f()))
    except ValueError:
        print("ValueError")

show(lambda: float("1_000.5"))
show(lambda: float("1_000"))
show(lambda: float("1_0.5e1_0"))
show(lambda: float("_1"))
show(lambda: float("1_"))
show(lambda: float("1__0"))
show(lambda: float("1_.0"))
show(lambda: float("1._0"))
show(lambda: float("3.14"))
show(lambda: float("  2.5  "))
