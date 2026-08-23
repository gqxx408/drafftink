fn main() {
    // prost-build disabled for now — hand-rolled types avoid compile-time dependency
    // When ready, uncomment:
    // prost_build::compile_protos(&["proto/quiz.proto"], &["proto/"]).unwrap();
    println!("cargo:rerun-if-changed=proto/quiz.proto");
}