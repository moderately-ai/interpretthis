# Mixed-builtin arithmetic coerces to the widest type per CPython's rules:
# any-float → float, otherwise → int. bool participates as 0/1.
print(1 + 1.0)
print(1.5 - 1)
print(2 * 1.5)
print(True + 1)
print(True * 2.5)
print(1 + True + False)
print(type(1 + 1.0).__name__)
print(type(True + 1).__name__)
print(type(1.5 + 1).__name__)
