# yield from delegates: each value from the inner iterable yields out
# through the outer generator. Pins Expr::YieldFrom + dispatch_iter
# integration in eval/mod.rs.
def inner():
    yield 1
    yield 2
    yield 3

def outer():
    yield 0
    yield from inner()
    yield 4

print(list(outer()))

# yield from a list literal
def yields_list():
    yield from [10, 20, 30]

print(list(yields_list()))

# yield from a range
def yields_range():
    yield from range(5)

print(list(yields_range()))

# Nested yield from
def grandchild():
    yield "a"
    yield "b"

def child():
    yield from grandchild()
    yield "c"

def parent():
    yield from child()
    yield "d"

print(list(parent()))
