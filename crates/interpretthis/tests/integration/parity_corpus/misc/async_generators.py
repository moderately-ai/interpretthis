import asyncio


async def agen(n):
    for i in range(n):
        yield i


async def main():
    result = []
    async for x in agen(4):
        result.append(x)
    return result


print(asyncio.run(main()))


async def squares(n):
    for i in range(n):
        yield i * i


async def collect():
    return [x async for x in squares(5)]


print(asyncio.run(collect()))


async def countdown(start):
    while start > 0:
        yield start
        start -= 1


async def use_countdown():
    return [n async for n in countdown(3)]


print(asyncio.run(use_countdown()))


async def with_break():
    result = []
    async for x in agen(100):
        if x >= 3:
            break
        result.append(x)
    return result


print(asyncio.run(with_break()))
