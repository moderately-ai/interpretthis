# `raise X from Y` sets the new exception's __cause__ to Y so the
# traceback shows "The above exception was the direct cause of the
# following exception". Probes that we correctly propagate __cause__
# through the exception chain.
try:
    try:
        raise ValueError("inner")
    except ValueError as e:
        raise RuntimeError("outer") from e
except RuntimeError as r:
    print(type(r).__name__, str(r))
    print(type(r.__cause__).__name__, str(r.__cause__))
