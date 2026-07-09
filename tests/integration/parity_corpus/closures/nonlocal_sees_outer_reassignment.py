# If outer reassigns a name that an inner has declared `nonlocal`
# between inner's def and inner's call, the inner must see the new
# value on its next call. CPython binds inner's nonlocal to outer's
# actual cell object; reassignment in outer mutates that cell.
def outer():
    n = 10
    def inner():
        nonlocal n
        n = n + 1
        return n
    n = 20
    return inner()

print(outer())
