# Basic generator function: yield in sequence. Pins eval_function_def's
# generator detection + Expr::Yield arm + the eager collection in
# call_user_function (Track C).
def count_up_to(n):
    i = 0
    while i < n:
        yield i
        i += 1

# for-loop over a generator
for x in count_up_to(3):
    print(x)

# list() materializes a generator
print(list(count_up_to(5)))

# sum() works because the generator iterates to completion
print(sum(count_up_to(10)))

# Empty generator
def empty():
    return

print(list(empty()))

def yields_strings():
    yield "a"
    yield "b"
    yield "c"

print(list(yields_strings()))
print("-".join(yields_strings()))
