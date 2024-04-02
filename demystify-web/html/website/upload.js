// Add event listener to the button element
const uploadButton = document.getElementById("uploadButton");
uploadButton.addEventListener("click", uploadFiles);

function uploadFiles(event) {
  event.preventDefault();

  const fileInput = document.getElementById("fileInput");
  const selectedFiles = fileInput.files;

  const formData = new FormData();
  for (let i = 0; i < selectedFiles.length; i++) {
    formData.append("files[]", selectedFiles[i]);
  }

  const xhr = new XMLHttpRequest();
  xhr.open("POST", "/uploadPuzzle", true);
  xhr.onreadystatechange = function () {
    console.log(xhr);
    const uploadMessage = document.getElementById("uploadMessage");
    if (xhr.readyState === XMLHttpRequest.DONE) {
      if(xhr.status == 200) {
          // Handle successful response from the server
          console.log("Files uploaded successfully!");
          uploadMessage.textContent = "Upload successful";
      }
      else {
          // Handle error response from the server
          console.error("Failed to upload files: ", xhr.responseText);
          uploadMessage.textContent = "Upload failed: " + xhr.responseText;
        
      }
    } else {
      uploadMessage.textContent = "Uploading...";
    }
  };
  xhr.send(formData);
}
