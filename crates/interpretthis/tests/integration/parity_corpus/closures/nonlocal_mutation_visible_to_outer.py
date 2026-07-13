# Inner function declares `nonlocal x` and mutates x; the outer
# function then reads x and must see the inner's update. CPython
# semantics: nonlocal binds inner's x to the same variable in the
# enclosing scope, so mutations propagate up while the outer is
# still on the stack.
def outer():
    x = 5
    def inner():
        nonlocal x
        x = x + 1
    inner()
    return x

print(outer())
