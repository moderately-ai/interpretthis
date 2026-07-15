def countdown(n):
    while n > 0:
        yield n
        n -= 1
print(list(countdown(5)))
def take(gen, k):
    result = []
    for _ in range(k):
        result.append(next(gen))
    return result
def naturals():
    n = 1
    while True:
        yield n
        n += 1
print(take(naturals(), 5))
def fib():
    a, b = 0, 1
    while True:
        yield a
        a, b = b, a + b
g = fib()
print([next(g) for _ in range(10)])
def repeat_while(x, times):
    i = 0
    while i < times:
        yield x
        i += 1
print(list(repeat_while("a", 3)))
