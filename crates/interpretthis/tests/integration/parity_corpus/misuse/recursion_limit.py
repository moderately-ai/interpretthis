def infinite(n):
    return infinite(n + 1)
try:
    infinite(0)
except RecursionError:
    print("recursion error caught")
