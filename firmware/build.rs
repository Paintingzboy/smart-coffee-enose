fn main() {
    embuild::espidf::sysenv::output();
    // TODO: Saat Edge Impulse library sudah di-export ke folder ei_library/,
    // aktifkan baris di bawah untuk compile C++ library:
    //
    // println!("cargo:rerun-if-changed=ei_library/");
    // cc::Build::new()
    //     .cpp(true)
    //     .flag("-std=c++11")
    //     .flag("-DEI_CLASSIFIER_TFLITE_ENABLE_ESP_NN=1")
    //     .include("ei_library/edge-impulse-sdk")
    //     .include("ei_library/model-parameters")
    //     .include("ei_library/tflite-model")
    //     .file("ei_library/tflite-model/tflite-trained.cpp")
    //     // tambah semua .cpp dari EI library di sini
    //     .compile("edge_impulse");
}
