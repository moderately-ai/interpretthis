# A class body executes as a code block: names bound inside if/else/for/while
# (including loop variables) become class attributes, reads fall through to the
# enclosing scope, and a nested class becomes an attribute. Regression: the
# class-body loop only handled FunctionDef/Assign/AnnAssign and dropped every
# control-flow statement via a `_ => {}` arm.
COND = True
LIMIT = 3


class C:
    if COND:
        x = 1
    else:
        x = 99
    for i in range(LIMIT):
        pass
    total = 0
    for j in range(LIMIT):
        total = total + j
    while total < 10:
        total = total + 5


print(C.x)
print(C.i)          # loop variable leaks into the class namespace
print(C.total)
print(hasattr(C, "j"))


class Outer:
    tag = "outer"

    class Inner:
        tag = "inner"


print(Outer.tag, Outer.Inner.tag)


# A class attribute built up across a loop, reading an earlier class-body name.
class D:
    values = []
    for k in range(3):
        values = values + [k * k]


print(D.values)


# The enclosing name is read, not shadowed, when the body only reads it.
FACTOR = 10


class E:
    scaled = [n * FACTOR for n in range(3)]
    if FACTOR > 5:
        big = True


print(E.scaled, E.big)
