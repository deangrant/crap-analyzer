package main

func trivial() {
	x := 1
	_ = x
}

func branched(x int) int {
	if x > 0 {
		return x
	}
	return 0
}

func main() {
	_ = trivial()
	_ = branched(1)
}
