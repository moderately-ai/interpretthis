def factorial(n):
    return 1 if n <= 1 else n * factorial(n - 1)
print(factorial(100))
def fib(n, memo={}):
    if n in memo:
        return memo[n]
    if n < 2:
        return n
    memo[n] = fib(n-1) + fib(n-2)
    return memo[n]
print(fib(50))
def ackermann(m, n):
    if m == 0:
        return n + 1
    if n == 0:
        return ackermann(m - 1, 1)
    return ackermann(m - 1, ackermann(m, n - 1))
print(ackermann(2, 3))
def sum_to(n, acc=0):
    if n == 0:
        return acc
    return sum_to(n - 1, acc + n)
print(sum_to(500))
def count_down(n):
    if n <= 0:
        return []
    return [n] + count_down(n - 1)
print(len(count_down(100)))
