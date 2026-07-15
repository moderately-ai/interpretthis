def gen_with_except():
    try:
        yield 1
        yield 2
    except GeneratorExit:
        print("cleanup on close")
        raise
g = gen_with_except()
print(next(g))
g.close()
def gen_throw_handle():
    try:
        yield 1
    except ValueError as e:
        yield f"caught {e}"
    yield "done"
g2 = gen_throw_handle()
print(next(g2))
print(g2.throw(ValueError("x")))
print(next(g2))
def gen_finally_close():
    try:
        yield 1
        yield 2
    finally:
        print("finally on close")
g3 = gen_finally_close()
print(next(g3))
g3.close()
