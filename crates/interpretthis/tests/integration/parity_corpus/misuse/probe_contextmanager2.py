from contextlib import contextmanager, suppress
@contextmanager
def tag(name):
    print(f"<{name}>")
    yield name
    print(f"</{name}>")
with tag("div") as t:
    print(f"content of {t}")
with suppress(ValueError):
    raise ValueError("ignored")
print("after suppress")
@contextmanager
def resource():
    r = {"open": True}
    try:
        yield r
    finally:
        r["open"] = False
with resource() as res:
    print(res["open"])
print(res["open"])
