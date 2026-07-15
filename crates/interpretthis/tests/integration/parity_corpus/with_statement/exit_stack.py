from contextlib import ExitStack


class Resource:
    def __init__(self, name):
        self.name = name

    def __enter__(self):
        print(f"enter {self.name}")
        return self

    def __exit__(self, *args):
        print(f"exit {self.name}")
        return False


with ExitStack() as stack:
    a = stack.enter_context(Resource("A"))
    b = stack.enter_context(Resource("B"))
    print(f"using {a.name} {b.name}")
print("done")

# Callbacks unwind in LIFO order.
with ExitStack() as stack:
    stack.callback(lambda: print("cb1"))
    stack.callback(print, "cb2 with arg")
    print("in block")
print("after")

# Mixed context managers and callbacks.
with ExitStack() as stack:
    stack.enter_context(Resource("X"))
    stack.callback(lambda: print("cleanup"))
    stack.enter_context(Resource("Y"))
    print("mixed block")
print("mixed done")

# close() runs cleanups explicitly.
es = ExitStack()
es.enter_context(Resource("Z"))
es.callback(lambda: print("explicit cb"))
print("before close")
es.close()
print("closed")

# An exception in the block still unwinds the stack.
try:
    with ExitStack() as stack:
        stack.callback(lambda: print("cleanup on error"))
        stack.enter_context(Resource("E"))
        raise ValueError("boom")
except ValueError as e:
    print("caught", str(e))
