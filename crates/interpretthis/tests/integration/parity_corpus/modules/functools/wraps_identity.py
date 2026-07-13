# functools.wraps as a no-op identity decorator. Pins the wraps()
# return-the-wrapped semantic.
import functools

def make_doubler(fn):
    @functools.wraps(fn)
    def inner_double(x):
        return fn(x) * 2
    return inner_double

@make_doubler
def add_one(x):
    return x + 1

print(add_one(5))   # (5 + 1) * 2 = 12
print(add_one(0))   # (0 + 1) * 2 = 2
