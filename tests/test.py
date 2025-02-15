import argparse
import time


def main(args):
    print("Hello, World!")
    print(args)
    time.sleep(5)
    print("Goodbye, World!")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--test", help="Test argument")
    args = parser.parse_args()
    main(args)
