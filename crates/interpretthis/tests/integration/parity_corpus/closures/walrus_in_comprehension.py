# PEP 572: a walrus inside a comprehension's element or filter binds
# to the COMPREHENSION's enclosing function scope. Inside a function,
# the walrus target must NOT leak to module scope after the function
# returns. (Walrus in the iter position is a CPython SyntaxError, so
# we use the element-position case here.)
def g():
    data = [1, 2, 3, 4, 5]
    out = [doubled for x in data if (doubled := x * 2) > 4]
    return out, doubled

result, last = g()
print(result)
print(last)
try:
    print(doubled)
except NameError as e:
    print(f"NameError: {e}")
