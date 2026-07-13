# Inside a function, a nested function that mutates an outer-local
# mutable container (without declaring nonlocal) must propagate the
# mutation to the outer. CPython binds inner's free `items` to
# outer's local cell, which holds a reference to the list object.
def outer():
    items = []
    def inner():
        items.append(1)
    inner()
    return items

print(outer())
