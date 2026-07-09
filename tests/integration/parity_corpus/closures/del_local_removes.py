# `del x` inside a function removes x from the LOCAL scope only;
# the outer scope's binding of the same name must not be affected.
# Probes the checkpoint's handling of names introduced AND removed
# inside the same frame.
x = "outer"

def inner():
    x = "inner"
    del x
    # x is now unbound LOCALLY; reading it now would NameError —
    # not "outer".

inner()
print(x)
