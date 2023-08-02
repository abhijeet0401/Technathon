from flask import Flask, jsonify, request
from flask_restful import Api,Resource
from pymongo import MongoClient
import pickle
import numpy as np
app = Flask(__name__)
api = Api(app)

class HeartBeatCheck(Resource):
    def post(self):
        postedData = request.get_json()

        age = postedData["age"]
        heartBeat = postedData["heartBeat"]

        with open('modelnaive.pkl', 'rb') as f:
            clf3 = pickle.load(f)

        data = np.array([int(age),int(heartBeat)]).tolist()
        heartbeat_prediction = clf3.predict([data]).tolist()[0]

        retJson = {
        "Status" : 200,
        "Result" : heartbeat_prediction,
        "Message": "Succesfully loged In"
        }
        return jsonify(retJson)
api.add_resource(HeartBeatCheck,"/heart_beat_check")

if __name__ =="__main__":
    app.run(host='0.0.0.0')
