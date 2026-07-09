# The `i=i` idiom is the canonical workaround for the "all lambdas
# capture the last loop value" closure trap. It depends on CPython
# evaluating the default expression at def time — so each lambda
# binds its own i. Without correct def-time evaluation, every
# lambda sees the same final i (or worse, NameError on call when
# the loop has ended and i is out of scope).
funcs = [lambda x, i=i: x + i for i in range(3)]
print([f(10) for f in funcs])
