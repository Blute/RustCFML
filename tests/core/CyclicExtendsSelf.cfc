// Fixture for core/test_recursion_and_depth_limits.cfm: a component that is
// its own parent. Only ever instantiated inside assertThrows().
component extends="core.CyclicExtendsSelf" {
}
