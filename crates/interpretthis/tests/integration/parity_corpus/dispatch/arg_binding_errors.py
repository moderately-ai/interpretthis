# A user function must reject a mis-shaped call, not silently ignore the excess.
# Regression: the binder iterated parameters and never checked the arguments, so
# extra positionals were dropped, a name passed both positionally and by keyword
# kept the positional and dropped the keyword, and unknown keywords vanished when
# there was no **kwargs. Every one returned a plausible-but-wrong value instead of
# raising TypeError — the single highest-frequency LLM mistake.
def f(a):
    return a


def g(a, b):
    return (a, b)


# Extra positional args (no *args to absorb them).
try:
    f(1, 2, 3)
except TypeError:
    print("too many positional")

# Same name positionally and by keyword.
try:
    f(1, a=2)
except TypeError:
    print("multiple values")

# Unknown keyword, no **kwargs.
try:
    f(1, b=99)
except TypeError:
    print("unexpected keyword")

# The valid calls still work.
print(f(1))
print(f(a=5))
print(g(1, b=2))
print(g(1, 2))


# *args and **kwargs still absorb the excess.
def variadic(a, *args, **kwargs):
    return (a, args, sorted(kwargs.items()))


print(variadic(1, 2, 3, x=4, y=5))
