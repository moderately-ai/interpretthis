# A generator with a top-level `while` loop suspends at each yield instead of
# eagerly buffering, so infinite generators compose with islice/takewhile etc.
import itertools


def fib():
    a, b = 0, 1
    while True:
        yield a
        a, b = b, a + b


print(list(itertools.islice(fib(), 10)))
print(list(itertools.islice(fib(), 2, 8)))
print(list(itertools.islice(fib(), 1, 10, 2)))


def counter(start):
    n = start
    while True:
        yield n
        n += 1


print(list(itertools.islice(counter(100), 5)))
print(list(itertools.takewhile(lambda x: x < 20, fib())))


# A bounded while generator still terminates and is fully consumable.
def countdown(n):
    while n > 0:
        yield n
        n -= 1


print(list(countdown(5)))
print(sum(countdown(4)))


# send() into a while generator resumes correctly.
def echo():
    while True:
        received = yield
        print(f"got {received}")


e = echo()
next(e)
e.send("a")
e.send("b")
