# Pins: lambda with default args; nested lambdas form closures.
f = lambda x, y=10: x + y
print(f(5))
print(f(5, 20))

adder = lambda x: lambda y: x + y
add5 = adder(5)
print(add5(3))
