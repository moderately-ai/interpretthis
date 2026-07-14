# A default argument may be any expression, evaluated once at def time.
# Regression: defaults were captured by unparsing the AST back to source, and
# anything outside a tiny set (Constant/Name/List/Tuple/Dict/UnaryOp/BinOp/Call)
# fell back to the source `None  # unparseable`, which parses to None. So
# `def f(x=CONFIG["n"])` silently defaulted x to None. These expression forms must
# now round-trip and evaluate correctly.
CONFIG = {"n": 5}


class Settings:
    limit = 7


def sub(x=CONFIG["n"]):
    return x


def attr(x=Settings.limit):
    return x


def ternary(x=10 if True else 20):
    return x


def boolean(x=0 or 42):
    return x


def comparison(x=1 < 2):
    return x


def setlit(x={3, 1, 2}):
    return sorted(x)


def lam(cb=lambda a: a + 1):
    return cb(10)


print(sub())
print(attr())
print(ternary())
print(boolean())
print(comparison())
print(setlit())
print(lam())

# Defaults are still evaluated once at def time (mutable-default sharing).
def accum(x, acc=[]):
    acc.append(x)
    return acc


print(accum(1))
print(accum(2))
