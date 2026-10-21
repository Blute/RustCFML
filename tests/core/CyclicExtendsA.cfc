// Fixture for core/test_recursion_and_depth_limits.cfm: A extends B extends A.
// Only ever instantiated inside assertThrows().
component extends="core.CyclicExtendsB" {
}
