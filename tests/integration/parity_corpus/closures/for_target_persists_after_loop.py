# In CPython, the for-loop target variable persists after the loop
# completes (it's the value of the last iteration). When the iterable
# is empty, the target is NOT bound — reading it raises NameError.
def loops():
    for i in range(3):
        pass
    # i should be 2 here (last iteration value).
    last = i

    # Inside its own scope:
    for j in []:
        pass
    # j is unbound because the loop body never executed.
    try:
        return last, j
    except NameError:
        return last, "unbound"

print(loops())
