def factorial(n):
    return 1 if n <= 1 else n * factorial(n - 1)
print(factorial(10))
def fib(n):
    return n if n < 2 else fib(n-1) + fib(n-2)
print(fib(15))
def gcd(a, b):
    return a if b == 0 else gcd(b, a % b)
print(gcd(48, 36))
def flatten(lst):
    result = []
    for item in lst:
        if isinstance(item, list):
            result.extend(flatten(item))
        else:
            result.append(item)
    return result
print(flatten([1, [2, [3, [4, 5]]], 6]))
def power(base, exp):
    return 1 if exp == 0 else base * power(base, exp - 1)
print(power(2, 10))
def sum_digits(n):
    return n if n < 10 else n % 10 + sum_digits(n // 10)
print(sum_digits(12345))
def ackermann(m, n):
    if m == 0: return n + 1
    if n == 0: return ackermann(m - 1, 1)
    return ackermann(m - 1, ackermann(m, n - 1))
print(ackermann(2, 3))
def count_down(n, acc=None):
    if acc is None: acc = []
    if n == 0: return acc
    acc.append(n)
    return count_down(n - 1, acc)
print(count_down(5))
