def g():
    try:
        yield 1
        yield 2
    finally:
        print("closed")
it = g()
print(next(it))
it.close()
print("after close")
